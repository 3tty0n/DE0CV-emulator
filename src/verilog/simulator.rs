use super::ast::*;
use crate::board::Board;
use std::collections::HashMap;

type SigId = usize;

// Compiled IR (signal-id based, no string lookups during simulation)

#[derive(Debug, Clone)]
enum CExpr {
    Const(u64),
    Sig(SigId),
    BitSel(SigId, Box<CExpr>),
    BinOp(Box<CExpr>, BinOp, Box<CExpr>),
    UnaryOp(UnaryOp, Box<CExpr>),
    Ternary(Box<CExpr>, Box<CExpr>, Box<CExpr>),
}

#[derive(Debug, Clone)]
enum CLValue {
    Sig(SigId),
    BitSel(SigId, Box<CExpr>),
}

/// Resolved NBA entry — no heap allocation
#[derive(Debug, Clone, Copy)]
enum NbaEntry {
    Sig(SigId, u64),        // write full signal
    BitSel(SigId, u64, u64), // signal id, bit index, value
}

#[derive(Debug, Clone)]
enum CStmt {
    Block(Vec<CStmt>),
    If {
        cond: CExpr,
        then: Box<CStmt>,
        else_: Option<Box<CStmt>>,
    },
    Case {
        expr: CExpr,
        arms: Vec<(Vec<u64>, CStmt)>,
        default: Option<Box<CStmt>>,
    },
    Blocking(CLValue, CExpr),
    NonBlocking(CLValue, CExpr),
}

struct SignalInfo {
    width: u32,
}

pub struct PortMapping {
    pub clk: Option<SigId>,
    pub rst: Option<SigId>,
    pub sw: Option<SigId>,
    pub key: Option<SigId>,
    pub ledr: Option<SigId>,
    pub hex: [Option<SigId>; 6],
    /// Non-standard output ports mapped to LEDs (name, sig_id, width)
    pub output_leds: Vec<(String, SigId, u32)>,
    /// Non-standard input ports mapped from switches (name, sig_id, width)
    pub input_sw: Vec<(String, SigId, u32)>,
}

pub struct Simulator {
    signals: Vec<SignalInfo>,
    values: Vec<u64>,
    prev_values: Vec<u64>,
    sequential: Vec<CStmt>,
    combinational: Vec<CStmt>,
    assigns: Vec<(CLValue, CExpr)>,
    pub ports: PortMapping,
    nba_queue: Vec<NbaEntry>,
    pub top_name: String,
    sig_map: HashMap<String, SigId>,
}

impl Simulator {
    pub fn build(
        modules: &[VerilogModule],
        top_name: Option<&str>,
    ) -> Result<Self, String> {
        if modules.is_empty() {
            return Err("No modules found".to_string());
        }

        // Find top module
        let top = if let Some(name) = top_name {
            modules
                .iter()
                .find(|m| m.name == name)
                .ok_or_else(|| format!("Module '{}' not found", name))?
        } else {
            &modules[0]
        };

        // Build module lookup for instantiation
        let mod_map: HashMap<String, &VerilogModule> =
            modules.iter().map(|m| (m.name.clone(), m)).collect();

        let mut builder = SimBuilder {
            signals: Vec::new(),
            values: Vec::new(),
            sig_map: HashMap::new(),
            sequential: Vec::new(),
            combinational: Vec::new(),
            assigns: Vec::new(),
            mod_map,
        };

        builder.compile_module(top, "")?;

        // Resolve port mappings
        let mut ports = PortMapping {
            clk: builder.find_sig("CLK"),
            rst: builder.find_sig("RST"),
            sw: builder.find_sig("SW"),
            key: builder.find_sig("KEY"),
            ledr: builder.find_sig("LEDR"),
            hex: [
                builder.find_sig("HEX0"),
                builder.find_sig("HEX1"),
                builder.find_sig("HEX2"),
                builder.find_sig("HEX3"),
                builder.find_sig("HEX4"),
                builder.find_sig("HEX5"),
            ],
            output_leds: Vec::new(),
            input_sw: Vec::new(),
        };

        // Auto-map non-standard output ports to LEDs and input ports to switches
        let known_inputs = ["CLK", "RST", "SW", "KEY"];
        let known_outputs = ["LEDR", "HEX0", "HEX1", "HEX2", "HEX3", "HEX4", "HEX5"];
        for item in &top.items {
            if let ModuleItem::PortDecl(pd) = item {
                for name in &pd.names {
                    if pd.dir == PortDir::Output && !known_outputs.contains(&name.as_str()) {
                        if let Some(&id) = builder.sig_map.get(name) {
                            let w = builder.signals[id].width;
                            ports.output_leds.push((name.clone(), id, w));
                        }
                    } else if pd.dir == PortDir::Input && !known_inputs.contains(&name.as_str()) {
                        if let Some(&id) = builder.sig_map.get(name) {
                            let w = builder.signals[id].width;
                            ports.input_sw.push((name.clone(), id, w));
                        }
                    }
                }
            }
        }

        let prev_values = builder.values.clone();
        Ok(Simulator {
            signals: builder.signals,
            values: builder.values,
            prev_values,
            sequential: builder.sequential,
            combinational: builder.combinational,
            assigns: builder.assigns,
            ports,
            nba_queue: Vec::new(),
            top_name: top.name.clone(),
            sig_map: builder.sig_map,
        })
    }

    /// Run one clock cycle: posedge CLK → sequential → apply NBA → combinational settle
    #[inline]
    pub fn tick(&mut self) {
        // Save state before sequential eval to detect blocking assignment changes
        self.prev_values.copy_from_slice(&self.values);

        // Evaluate sequential blocks (posedge CLK)
        for i in 0..self.sequential.len() {
            let stmt = unsafe { &*(&self.sequential[i] as *const CStmt) };
            eval_stmt_fn(stmt, &mut self.values, &self.signals, &mut self.nba_queue);
        }

        // Check if blocking assignments changed any values
        let mut changed = self.values != self.prev_values;
        for i in 0..self.nba_queue.len() {
            match self.nba_queue[i] {
                NbaEntry::Sig(id, val) => {
                    let mask = if self.signals[id].width >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << self.signals[id].width) - 1
                    };
                    let new_val = val & mask;
                    if self.values[id] != new_val {
                        self.values[id] = new_val;
                        changed = true;
                    }
                }
                NbaEntry::BitSel(id, bit, val) => {
                    if bit < 64 {
                        let old = self.values[id];
                        let mask = 1u64 << bit;
                        if val & 1 != 0 {
                            self.values[id] |= mask;
                        } else {
                            self.values[id] &= !mask;
                        }
                        if self.values[id] != old {
                            changed = true;
                        }
                    }
                }
            }
        }
        self.nba_queue.clear();

        // Only re-settle combinational logic if sequential values changed
        if !changed {
            return;
        }
        for _ in 0..10 {
            self.prev_values.copy_from_slice(&self.values);
            for i in 0..self.assigns.len() {
                let pair = unsafe { &*(&self.assigns[i] as *const (CLValue, CExpr)) };
                let val = eval_expr_fn(&pair.1, &self.values);
                write_lvalue_fn(&pair.0, val, &mut self.values, &self.signals);
            }
            for i in 0..self.combinational.len() {
                let stmt = unsafe { &*(&self.combinational[i] as *const CStmt) };
                eval_stmt_fn(stmt, &mut self.values, &self.signals, &mut self.nba_queue);
            }
            if self.values == self.prev_values {
                break;
            }
        }
    }

    /// Force one round of combinational settle (needed at startup)
    pub fn settle(&mut self) {
        let assigns = std::mem::take(&mut self.assigns);
        let comb = std::mem::take(&mut self.combinational);
        for _ in 0..10 {
            self.prev_values.copy_from_slice(&self.values);
            for i in 0..assigns.len() {
                let val = eval_expr_fn(&assigns[i].1, &self.values);
                write_lvalue_fn(&assigns[i].0, val, &mut self.values, &self.signals);
            }
            for stmt in &comb {
                eval_stmt_fn(stmt, &mut self.values, &self.signals, &mut self.nba_queue);
            }
            if self.values == self.prev_values {
                break;
            }
        }
        self.assigns = assigns;
        self.combinational = comb;
    }

    /// Sync board inputs → simulator signals
    pub fn read_inputs(&mut self, board: &Board) {
        if let Some(id) = self.ports.rst {
            // RST is active when 'r' is pressed OR SW9 is on (SW9 = RST on DE0-CV)
            self.values[id] = if board.rst || board.sw[9] { 1 } else { 0 };
        }
        if let Some(id) = self.ports.sw {
            let mut val = 0u64;
            for (i, &s) in board.sw.iter().enumerate() {
                if s {
                    val |= 1 << i;
                }
            }
            self.values[id] = val;
        }
        if let Some(id) = self.ports.key {
            let mut val = 0u64;
            for (i, &k) in board.key.iter().enumerate() {
                if k {
                    val |= 1 << i;
                }
            }
            self.values[id] = val;
        }
        // Map switches to non-standard input ports (e.g., "data" for SR2)
        let mut sw_bit = 0usize;
        for (_, id, width) in &self.ports.input_sw {
            let mut val = 0u64;
            for b in 0..(*width as usize) {
                if sw_bit + b < board.sw.len() && board.sw[sw_bit + b] {
                    val |= 1 << b;
                }
            }
            self.values[*id] = val;
            sw_bit += *width as usize;
        }
    }

    /// Scan all continuous assigns for the largest `sig == N` constant and
    /// derive a cycles-per-frame value so the simulation runs in real time.
    /// Returns a fallback of 1_000 if no suitable constant is found.
    pub fn suggest_cycles_per_frame(&self) -> u64 {
        let mut max_n: u64 = 0;
        for (_, expr) in &self.assigns {
            let n = find_max_eq_const(expr);
            if n > max_n {
                max_n = n;
            }
        }
        if max_n > 0 {
            // max_n is the counter period - 1 (e.g. 49_999 for 50 kHz).
            // cycles_per_frame = (max_n + 1) / 60  so that 60 frames ≈ 1 second of simulated time.
            ((max_n + 1) / 60).max(1)
        } else {
            1_000
        }
    }

    /// Sync simulator signals → board outputs
    pub fn write_outputs(&self, board: &mut Board) {
        if let Some(id) = self.ports.ledr {
            let val = self.values[id];
            for i in 0..10 {
                board.ledr[i] = (val >> i) & 1 != 0;
            }
        }
        for i in 0..6 {
            if let Some(id) = self.ports.hex[i] {
                let val = self.values[id] as u8;
                board.set_hex(i, val);
            }
        }
        // Auto-display output_leds on 7-seg when no standard HEX ports are mapped
        // (e.g. DiceCounter outputs dice[2:0] with no HEX port)
        if self.ports.hex.iter().all(|h| h.is_none()) && !self.ports.output_leds.is_empty() {
            let mut total_val = 0u64;
            let mut bit_offset = 0u32;
            for (_, id, width) in &self.ports.output_leds {
                total_val |= self.values[*id] << bit_offset;
                bit_offset += *width;
            }
            for i in 0..6usize {
                let nibble = ((total_val >> (i * 4)) & 0xF) as u8;
                board.set_hex(i, nibble_to_seg7(nibble));
            }
        }
    }

}

// Free functions for evaluation — avoids borrow conflicts with Simulator fields

/// Recursively scan a CExpr tree for the largest constant N that appears in
/// a `Sig == Const(N)` equality check.  These constants are the counter
/// moduli used to generate enable strobes (e.g. `cnt == 26'd49_999`).
fn find_max_eq_const(expr: &CExpr) -> u64 {
    match expr {
        CExpr::Ternary(cond, t, f) => find_max_eq_const(cond)
            .max(find_max_eq_const(t))
            .max(find_max_eq_const(f)),
        CExpr::BinOp(l, op, r) => {
            let inner = find_max_eq_const(l).max(find_max_eq_const(r));
            if matches!(op, BinOp::Eq) {
                match (l.as_ref(), r.as_ref()) {
                    (CExpr::Sig(_), CExpr::Const(n)) | (CExpr::Const(n), CExpr::Sig(_)) => {
                        inner.max(*n)
                    }
                    _ => inner,
                }
            } else {
                inner
            }
        }
        CExpr::UnaryOp(_, e) => find_max_eq_const(e),
        CExpr::BitSel(_, idx) => find_max_eq_const(idx),
        _ => 0,
    }
}

fn eval_expr_fn(expr: &CExpr, values: &[u64]) -> u64 {
    match expr {
        CExpr::Const(v) => *v,
        CExpr::Sig(id) => values[*id],
        CExpr::BitSel(id, idx) => {
            let bit = eval_expr_fn(idx, values);
            (values[*id] >> bit) & 1
        }
        CExpr::BinOp(l, op, r) => {
            let lv = eval_expr_fn(l, values);
            let rv = eval_expr_fn(r, values);
            match op {
                BinOp::Add => lv.wrapping_add(rv),
                BinOp::Sub => lv.wrapping_sub(rv),
                BinOp::Eq => if lv == rv { 1 } else { 0 },
                BinOp::Neq => if lv != rv { 1 } else { 0 },
                BinOp::Lt => if lv < rv { 1 } else { 0 },
                BinOp::Gt => if lv > rv { 1 } else { 0 },
                BinOp::Lte => if lv <= rv { 1 } else { 0 },
                BinOp::Gte => if lv >= rv { 1 } else { 0 },
                BinOp::BitAnd => lv & rv,
                BinOp::BitOr => lv | rv,
                BinOp::BitXor => lv ^ rv,
                BinOp::LogAnd => if lv != 0 && rv != 0 { 1 } else { 0 },
                BinOp::LogOr => if lv != 0 || rv != 0 { 1 } else { 0 },
            }
        }
        CExpr::UnaryOp(op, e) => {
            let v = eval_expr_fn(e, values);
            match op {
                UnaryOp::BitNot => !v,
                UnaryOp::LogNot => if v == 0 { 1 } else { 0 },
            }
        }
        CExpr::Ternary(cond, t, f) => {
            if eval_expr_fn(cond, values) != 0 {
                eval_expr_fn(t, values)
            } else {
                eval_expr_fn(f, values)
            }
        }
    }
}

fn eval_stmt_fn(
    stmt: &CStmt,
    values: &mut Vec<u64>,
    signals: &[SignalInfo],
    nba_queue: &mut Vec<NbaEntry>,
) {
    match stmt {
        CStmt::Block(stmts) => {
            for s in stmts {
                eval_stmt_fn(s, values, signals, nba_queue);
            }
        }
        CStmt::If { cond, then, else_ } => {
            if eval_expr_fn(cond, values) != 0 {
                eval_stmt_fn(then, values, signals, nba_queue);
            } else if let Some(e) = else_ {
                eval_stmt_fn(e, values, signals, nba_queue);
            }
        }
        CStmt::Case { expr, arms, default } => {
            let val = eval_expr_fn(expr, values);
            let mut matched = false;
            for (patterns, body) in arms {
                if patterns.iter().any(|p| *p == val) {
                    eval_stmt_fn(body, values, signals, nba_queue);
                    matched = true;
                    break;
                }
            }
            if !matched {
                if let Some(d) = default {
                    eval_stmt_fn(d, values, signals, nba_queue);
                }
            }
        }
        CStmt::Blocking(lval, expr) => {
            let val = eval_expr_fn(expr, values);
            write_lvalue_fn(lval, val, values, signals);
        }
        CStmt::NonBlocking(lval, expr) => {
            let val = eval_expr_fn(expr, values);
            match lval {
                    CLValue::Sig(id) => nba_queue.push(NbaEntry::Sig(*id, val)),
                    CLValue::BitSel(id, idx) => {
                        let bit = eval_expr_fn(idx, values);
                        nba_queue.push(NbaEntry::BitSel(*id, bit, val));
                    }
                }
        }
    }
}

fn write_lvalue_fn(lval: &CLValue, val: u64, values: &mut Vec<u64>, signals: &[SignalInfo]) {
    match lval {
        CLValue::Sig(id) => {
            let mask = if signals[*id].width >= 64 {
                u64::MAX
            } else {
                (1u64 << signals[*id].width) - 1
            };
            values[*id] = val & mask;
        }
        CLValue::BitSel(id, idx) => {
            let bit = eval_expr_fn(idx, values);
            if bit < 64 {
                let mask = 1u64 << bit;
                if val & 1 != 0 {
                    values[*id] |= mask;
                } else {
                    values[*id] &= !mask;
                }
            }
        }
    }
}

// Builder that compiles AST into the indexed simulator

struct SimBuilder<'a> {
    signals: Vec<SignalInfo>,
    values: Vec<u64>,
    sig_map: HashMap<String, SigId>,
    sequential: Vec<CStmt>,
    combinational: Vec<CStmt>,
    assigns: Vec<(CLValue, CExpr)>,
    mod_map: HashMap<String, &'a VerilogModule>,
}

impl<'a> SimBuilder<'a> {
    fn find_sig(&self, name: &str) -> Option<SigId> {
        self.sig_map.get(name).copied()
    }

    fn get_or_create_sig(&mut self, name: &str, width: u32) -> SigId {
        if let Some(&id) = self.sig_map.get(name) {
            // Update width if larger
            if width > self.signals[id].width {
                self.signals[id].width = width;
            }
            return id;
        }
        let id = self.signals.len();
        self.signals.push(SignalInfo { width });
        self.values.push(0);
        self.sig_map.insert(name.to_string(), id);
        id
    }

    fn prefixed(&self, prefix: &str, name: &str) -> String {
        if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}.{}", prefix, name)
        }
    }

    fn compile_module(&mut self, module: &VerilogModule, prefix: &str) -> Result<(), String> {
        // First pass: collect all signal declarations and their widths
        let mut sig_widths: HashMap<String, u32> = HashMap::new();

        for item in &module.items {
            match item {
                ModuleItem::PortDecl(pd) => {
                    let w = range_to_width(pd.range);
                    for name in &pd.names {
                        sig_widths.insert(name.clone(), w);
                    }
                }
                ModuleItem::RegDecl(rd) => {
                    let w = range_to_width(rd.range);
                    for (name, _) in &rd.items {
                        sig_widths.insert(name.clone(), w);
                    }
                }
                ModuleItem::WireDecl(wd) => {
                    let w = range_to_width(wd.range);
                    for (name, _) in &wd.items {
                        sig_widths.insert(name.clone(), w);
                    }
                }
                _ => {}
            }
        }

        // Create signals
        for (name, width) in &sig_widths {
            let full_name = self.prefixed(prefix, name);
            self.get_or_create_sig(&full_name, *width);
        }

        // Second pass: compile items
        for item in &module.items {
            match item {
                ModuleItem::RegDecl(rd) => {
                    // Handle initial values
                    for (name, init) in &rd.items {
                        if let Some(init_expr) = init {
                            let full_name = self.prefixed(prefix, name);
                            let id = self.sig_map[&full_name];
                            if let Expr::Number(v, _) = init_expr {
                                self.values[id] = *v;
                            }
                        }
                    }
                }
                ModuleItem::WireDecl(wd) => {
                    // Handle wire initializers as continuous assigns
                    for (name, init) in &wd.items {
                        if let Some(init_expr) = init {
                            let full_name = self.prefixed(prefix, name);
                            let id = self.sig_map[&full_name];
                            let expr = self.compile_expr(init_expr, prefix, &sig_widths)
                                .map_err(|e| format!(
                                    "in module '{}', wire '{}': {}",
                                    module.name, name, e
                                ))?;
                            self.assigns.push((CLValue::Sig(id), expr));
                        }
                    }
                }
                ModuleItem::Assign(assign) => {
                    let lval = self.compile_lvalue(&assign.target, prefix, &sig_widths)
                        .map_err(|e| format!("in module '{}', assign statement: {}", module.name, e))?;
                    let expr = self.compile_expr(&assign.expr, prefix, &sig_widths)
                        .map_err(|e| format!("in module '{}', assign statement: {}", module.name, e))?;
                    self.assigns.push((lval, expr));
                }
                ModuleItem::Always(always) => {
                    let stmt = self.compile_stmt(&always.body, prefix, &sig_widths)
                        .map_err(|e| format!("in module '{}', always block: {}", module.name, e))?;
                    match &always.sensitivity {
                        Sensitivity::Star => {
                            self.combinational.push(stmt);
                        }
                        Sensitivity::Edges(_) => {
                            self.sequential.push(stmt);
                        }
                    }
                }
                ModuleItem::ModuleInst(inst) => {
                    self.compile_instantiation(inst, prefix, module, &sig_widths)?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn compile_instantiation(
        &mut self,
        inst: &ModuleInst,
        parent_prefix: &str,
        _parent: &VerilogModule,
        parent_sigs: &HashMap<String, u32>,
    ) -> Result<(), String> {
        let sub_module = self
            .mod_map
            .get(&inst.module_name)
            .ok_or_else(|| {
                let scope = if parent_prefix.is_empty() { "top level".to_string() }
                            else { format!("module '{}'", parent_prefix) };
                format!(
                    "module '{}' not found (referenced by instance '{}' in {})",
                    inst.module_name, inst.inst_name, scope
                )
            })?
            .clone();

        let inst_prefix = if parent_prefix.is_empty() {
            inst.inst_name.clone()
        } else {
            format!("{}.{}", parent_prefix, inst.inst_name)
        };

        // Compile the sub-module with instance prefix
        self.compile_module(sub_module, &inst_prefix)?;

        // Wire up port connections (positional)
        for (i, conn_expr) in inst.connections.iter().enumerate() {
            if i >= sub_module.port_names.len() {
                break;
            }
            let formal_port = &sub_module.port_names[i];
            let formal_full = self.prefixed(&inst_prefix, formal_port);

            // Determine port direction
            let is_input = sub_module.items.iter().any(|item| {
                if let ModuleItem::PortDecl(pd) = item {
                    pd.dir == PortDir::Input && pd.names.contains(formal_port)
                } else {
                    false
                }
            });

            if is_input {
                // Input: actual → formal (continuous assign)
                let formal_id = self.sig_map[&formal_full];
                let actual_expr = self.compile_expr(conn_expr, parent_prefix, parent_sigs)?;
                self.assigns.push((CLValue::Sig(formal_id), actual_expr));
            } else {
                // Output: formal → actual (continuous assign)
                let formal_id = self.sig_map[&formal_full];
                if let Expr::Ident(actual_name) = conn_expr {
                    let actual_full = self.prefixed(parent_prefix, actual_name);
                    if let Some(&actual_id) = self.sig_map.get(&actual_full) {
                        self.assigns.push((CLValue::Sig(actual_id), CExpr::Sig(formal_id)));
                    }
                }
            }
        }

        Ok(())
    }

    fn compile_expr(
        &self,
        expr: &Expr,
        prefix: &str,
        sigs: &HashMap<String, u32>,
    ) -> Result<CExpr, String> {
        match expr {
            Expr::Number(v, _) => Ok(CExpr::Const(*v)),
            Expr::Ident(name) => {
                let full = self.prefixed(prefix, name);
                if let Some(&id) = self.sig_map.get(&full) {
                    Ok(CExpr::Sig(id))
                } else {
                    // Try without prefix (for cross-scope references)
                    if let Some(&id) = self.sig_map.get(name) {
                        Ok(CExpr::Sig(id))
                    } else {
                        Err(format!("undefined signal '{}'", name))
                    }
                }
            }
            Expr::BitSelect(base, idx) => {
                if let Expr::Ident(name) = base.as_ref() {
                    let full = self.prefixed(prefix, name);
                    let id = self
                        .sig_map
                        .get(&full)
                        .or_else(|| self.sig_map.get(name))
                        .ok_or_else(|| format!("undefined signal '{}'", name))?;
                    let cidx = self.compile_expr(idx, prefix, sigs)?;
                    Ok(CExpr::BitSel(*id, Box::new(cidx)))
                } else {
                    Err("Complex bit select base not supported".to_string())
                }
            }
            Expr::BinOp(l, op, r) => {
                let cl = self.compile_expr(l, prefix, sigs)?;
                let cr = self.compile_expr(r, prefix, sigs)?;
                Ok(CExpr::BinOp(Box::new(cl), *op, Box::new(cr)))
            }
            Expr::UnaryOp(op, e) => {
                let ce = self.compile_expr(e, prefix, sigs)?;
                Ok(CExpr::UnaryOp(*op, Box::new(ce)))
            }
            Expr::Ternary(c, t, f) => {
                let cc = self.compile_expr(c, prefix, sigs)?;
                let ct = self.compile_expr(t, prefix, sigs)?;
                let cf = self.compile_expr(f, prefix, sigs)?;
                Ok(CExpr::Ternary(Box::new(cc), Box::new(ct), Box::new(cf)))
            }
        }
    }

    fn compile_lvalue(
        &self,
        lval: &LValue,
        prefix: &str,
        sigs: &HashMap<String, u32>,
    ) -> Result<CLValue, String> {
        match lval {
            LValue::Ident(name) => {
                let full = self.prefixed(prefix, name);
                let id = self
                    .sig_map
                    .get(&full)
                    .or_else(|| self.sig_map.get(name))
                    .ok_or_else(|| format!("undefined signal '{}' in lvalue", name))?;
                Ok(CLValue::Sig(*id))
            }
            LValue::BitSelect(name, idx) => {
                let full = self.prefixed(prefix, name);
                let id = self
                    .sig_map
                    .get(&full)
                    .or_else(|| self.sig_map.get(name))
                    .ok_or_else(|| format!("undefined signal '{}' in lvalue", name))?;
                let cidx = self.compile_expr(idx, prefix, sigs)?;
                Ok(CLValue::BitSel(*id, Box::new(cidx)))
            }
            LValue::RangeSelect(name, _, _) => {
                // Treat range select as full signal write
                let full = self.prefixed(prefix, name);
                let id = self
                    .sig_map
                    .get(&full)
                    .or_else(|| self.sig_map.get(name))
                    .ok_or_else(|| format!("undefined signal '{}' in lvalue", name))?;
                Ok(CLValue::Sig(*id))
            }
        }
    }

    fn compile_stmt(
        &self,
        stmt: &Statement,
        prefix: &str,
        sigs: &HashMap<String, u32>,
    ) -> Result<CStmt, String> {
        match stmt {
            Statement::Block(stmts) => {
                let compiled: Result<Vec<_>, _> = stmts
                    .iter()
                    .map(|s| self.compile_stmt(s, prefix, sigs))
                    .collect();
                Ok(CStmt::Block(compiled?))
            }
            Statement::If { cond, then, else_ } => {
                let cc = self.compile_expr(cond, prefix, sigs)?;
                let ct = self.compile_stmt(then, prefix, sigs)?;
                let ce = match else_ {
                    Some(e) => Some(Box::new(self.compile_stmt(e, prefix, sigs)?)),
                    None => None,
                };
                Ok(CStmt::If {
                    cond: cc,
                    then: Box::new(ct),
                    else_: ce,
                })
            }
            Statement::Case {
                expr,
                arms,
                default,
            } => {
                let ce = self.compile_expr(expr, prefix, sigs)?;
                let mut compiled_arms = Vec::new();
                for (patterns, body) in arms {
                    let mut vals = Vec::new();
                    for p in patterns {
                        if let Expr::Number(v, _) = p {
                            vals.push(*v);
                        }
                    }
                    let cb = self.compile_stmt(body, prefix, sigs)?;
                    compiled_arms.push((vals, cb));
                }
                let cd = match default {
                    Some(d) => Some(Box::new(self.compile_stmt(d, prefix, sigs)?)),
                    None => None,
                };
                Ok(CStmt::Case {
                    expr: ce,
                    arms: compiled_arms,
                    default: cd,
                })
            }
            Statement::Blocking(lval, expr) => {
                let cl = self.compile_lvalue(lval, prefix, sigs)?;
                let ce = self.compile_expr(expr, prefix, sigs)?;
                Ok(CStmt::Blocking(cl, ce))
            }
            Statement::NonBlocking(lval, expr) => {
                let cl = self.compile_lvalue(lval, prefix, sigs)?;
                let ce = self.compile_expr(expr, prefix, sigs)?;
                Ok(CStmt::NonBlocking(cl, ce))
            }
        }
    }
}

fn range_to_width(range: Option<(i32, i32)>) -> u32 {
    match range {
        Some((high, low)) => (high - low + 1).max(1) as u32,
        None => 1,
    }
}

/// Map a 4-bit nibble to active-low 7-segment encoding (gfedcba)
/// Matches the DE0-CV standard seg7dec encoding used in the assignments
fn nibble_to_seg7(n: u8) -> u8 {
    match n & 0xF {
        0x0 => 0b1000000,
        0x1 => 0b1111001,
        0x2 => 0b0100100,
        0x3 => 0b0110000,
        0x4 => 0b0011001,
        0x5 => 0b0010010,
        0x6 => 0b0000010,
        0x7 => 0b1011000,
        0x8 => 0b0000000,
        0x9 => 0b0010000,
        0xA => 0b0001000,
        0xB => 0b0000011,
        0xC => 0b1000110,
        0xD => 0b0100001,
        0xE => 0b0000110,
        _   => 0b0001110, // 0xF
    }
}
