use super::ast::*;
use super::lexer::Token;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof);
        self.pos += 1;
        tok
    }

    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        let tok = self.advance();
        if &tok == expected {
            Ok(())
        } else {
            Err(format!("Expected {:?}, got {:?} at pos {}", expected, tok, self.pos))
        }
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.advance() {
            Token::Ident(s) => Ok(s),
            tok => Err(format!("Expected identifier, got {:?} at pos {}", tok, self.pos)),
        }
    }

    pub fn parse_file(&mut self) -> Result<Vec<VerilogModule>, String> {
        let mut modules = Vec::new();
        while *self.peek() != Token::Eof {
            if *self.peek() == Token::Module {
                modules.push(self.parse_module()?);
            } else {
                self.advance(); // skip unknown top-level tokens
            }
        }
        Ok(modules)
    }

    fn parse_module(&mut self) -> Result<VerilogModule, String> {
        self.expect(&Token::Module)?;
        let name = self.expect_ident()?;

        // Parse port list
        self.expect(&Token::LParen)?;
        let (port_names, ansi_ports) = self.parse_port_list()?;
        self.expect(&Token::RParen)?;
        self.expect(&Token::Semi)?;

        let mut items: Vec<ModuleItem> = Vec::new();

        // Add ANSI port declarations
        for p in ansi_ports {
            items.push(ModuleItem::PortDecl(p));
        }

        // Parse module body
        while *self.peek() != Token::Endmodule && *self.peek() != Token::Eof {
            if let Some(item) = self.parse_module_item()? {
                items.push(item);
            }
        }
        self.expect(&Token::Endmodule)?;

        Ok(VerilogModule {
            name,
            port_names,
            items,
        })
    }

    /// Parse port list. Returns (port names in order, ANSI port declarations if any)
    fn parse_port_list(&mut self) -> Result<(Vec<String>, Vec<PortDecl>), String> {
        let mut names = Vec::new();
        let mut decls = Vec::new();

        if *self.peek() == Token::RParen {
            return Ok((names, decls));
        }

        // Check if ANSI style (starts with input/output)
        let is_ansi = matches!(self.peek(), Token::Input | Token::Output);

        if is_ansi {
            loop {
                let dir = match self.peek() {
                    Token::Input => {
                        self.advance();
                        PortDir::Input
                    }
                    Token::Output => {
                        self.advance();
                        PortDir::Output
                    }
                    _ => return Err(format!("Expected input/output, got {:?}", self.peek())),
                };

                let mut kind = None;
                if *self.peek() == Token::Wire {
                    kind = Some(SignalKind::Wire);
                    self.advance();
                } else if *self.peek() == Token::Reg {
                    kind = Some(SignalKind::Reg);
                    self.advance();
                }

                let range = self.try_parse_range()?;

                // Parse comma-separated names for this direction
                let mut port_names = Vec::new();
                let first_name = self.expect_ident()?;
                port_names.push(first_name);

                while *self.peek() == Token::Comma {
                    self.advance();
                    // Check if next is a new direction keyword or another name
                    if matches!(self.peek(), Token::Input | Token::Output) {
                        break;
                    }
                    let name = self.expect_ident()?;
                    port_names.push(name);
                }

                names.extend(port_names.clone());
                decls.push(PortDecl {
                    dir,
                    kind,
                    range,
                    names: port_names,
                });

                if *self.peek() == Token::RParen {
                    break;
                }
            }
        } else {
            // Non-ANSI: just identifier list
            let first = self.expect_ident()?;
            names.push(first);
            while *self.peek() == Token::Comma {
                self.advance();
                let name = self.expect_ident()?;
                names.push(name);
            }
        }

        Ok((names, decls))
    }

    fn try_parse_range(&mut self) -> Result<Option<(i32, i32)>, String> {
        if *self.peek() != Token::LBrack {
            return Ok(None);
        }
        self.advance(); // [
        let high = self.parse_const_number()?;
        self.expect(&Token::Colon)?;
        let low = self.parse_const_number()?;
        self.expect(&Token::RBrack)?;
        Ok(Some((high, low)))
    }

    fn parse_const_number(&mut self) -> Result<i32, String> {
        match self.advance() {
            Token::Number(v, _) => Ok(v as i32),
            tok => Err(format!("Expected number, got {:?}", tok)),
        }
    }

    fn parse_module_item(&mut self) -> Result<Option<ModuleItem>, String> {
        match self.peek().clone() {
            Token::Input | Token::Output => {
                let decl = self.parse_port_decl()?;
                Ok(Some(ModuleItem::PortDecl(decl)))
            }
            Token::Reg => {
                let decl = self.parse_reg_decl()?;
                Ok(Some(ModuleItem::RegDecl(decl)))
            }
            Token::Wire => {
                let decl = self.parse_wire_decl()?;
                Ok(Some(decl))
            }
            Token::Assign => {
                let assign = self.parse_assign()?;
                Ok(Some(ModuleItem::Assign(assign)))
            }
            Token::Always => {
                match self.parse_always()? {
                    Some(always) => Ok(Some(ModuleItem::Always(always))),
                    None => Ok(None), // skipped testbench always #delay
                }
            }
            Token::Ident(ref name) if name == "__initial__" => {
                // Skip initial block
                self.advance(); // consume __initial__
                self.skip_statement();
                Ok(None)
            }
            Token::Ident(_) => {
                // Could be module instantiation
                let inst = self.parse_module_inst()?;
                Ok(Some(ModuleItem::ModuleInst(inst)))
            }
            _ => {
                self.advance(); // skip unknown
                Ok(None)
            }
        }
    }

    fn parse_port_decl(&mut self) -> Result<PortDecl, String> {
        let dir = match self.advance() {
            Token::Input => PortDir::Input,
            Token::Output => PortDir::Output,
            tok => return Err(format!("Expected input/output, got {:?}", tok)),
        };

        let mut kind = None;
        if *self.peek() == Token::Wire {
            kind = Some(SignalKind::Wire);
            self.advance();
        } else if *self.peek() == Token::Reg {
            kind = Some(SignalKind::Reg);
            self.advance();
        }

        let range = self.try_parse_range()?;

        let mut names = Vec::new();
        names.push(self.expect_ident()?);
        while *self.peek() == Token::Comma {
            self.advance();
            names.push(self.expect_ident()?);
        }
        self.expect(&Token::Semi)?;

        Ok(PortDecl {
            dir,
            kind,
            range,
            names,
        })
    }

    fn parse_reg_decl(&mut self) -> Result<RegDecl, String> {
        self.expect(&Token::Reg)?;
        let range = self.try_parse_range()?;

        let mut items = Vec::new();
        loop {
            let name = self.expect_ident()?;
            let init = if *self.peek() == Token::Eq {
                self.advance();
                Some(self.parse_expr()?)
            } else {
                None
            };
            items.push((name, init));
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(&Token::Semi)?;

        Ok(RegDecl { range, items })
    }

    fn parse_wire_decl(&mut self) -> Result<ModuleItem, String> {
        self.expect(&Token::Wire)?;
        let range = self.try_parse_range()?;

        let mut items = Vec::new();
        loop {
            let name = self.expect_ident()?;
            let init = if *self.peek() == Token::Eq {
                self.advance();
                let expr = self.parse_expr()?;
                Some(expr)
            } else {
                None
            };
            items.push((name, init));
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(&Token::Semi)?;

        // Return as WireDecl; initializers are handled by the simulator as assigns
        Ok(ModuleItem::WireDecl(WireDecl {
            range,
            items,
        }))
    }

    fn parse_assign(&mut self) -> Result<Assign, String> {
        self.expect(&Token::Assign)?;

        // Skip optional delay (#number)
        if *self.peek() == Token::Hash {
            self.advance();
            // Skip the number
            match self.peek() {
                Token::Number(_, _) => {
                    self.advance();
                }
                _ => {}
            }
        }

        let target = self.parse_lvalue()?;
        self.expect(&Token::Eq)?;
        let expr = self.parse_expr()?;
        self.expect(&Token::Semi)?;

        Ok(Assign { target, expr })
    }

    fn parse_always(&mut self) -> Result<Option<AlwaysBlock>, String> {
        self.expect(&Token::Always)?;

        // Handle `always #delay stmt;` (testbench clock gen) — skip it
        if *self.peek() == Token::Hash {
            self.advance(); // #
            if matches!(self.peek(), Token::Number(_, _)) {
                self.advance(); // delay value
            }
            // Skip the following statement up to semicolon
            self.skip_statement();
            return Ok(None);
        }

        let sensitivity = self.parse_sensitivity()?;
        let body = self.parse_statement()?;

        Ok(Some(AlwaysBlock { sensitivity, body }))
    }

    /// Skip tokens until we consume a semicolon (for skipping unsupported statements)
    fn skip_statement(&mut self) {
        let mut depth = 0;
        loop {
            match self.advance() {
                Token::Begin => depth += 1,
                Token::End => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                    if depth == 0 {
                        return;
                    }
                }
                Token::Semi if depth == 0 => return,
                Token::Eof => return,
                _ => {}
            }
        }
    }

    fn parse_sensitivity(&mut self) -> Result<Sensitivity, String> {
        self.expect(&Token::At)?;

        match self.peek() {
            Token::Star => {
                self.advance();
                Ok(Sensitivity::Star)
            }
            Token::LParen => {
                self.advance();
                if *self.peek() == Token::Star {
                    self.advance();
                    self.expect(&Token::RParen)?;
                    return Ok(Sensitivity::Star);
                }

                let mut edges = Vec::new();
                loop {
                    if *self.peek() == Token::Posedge {
                        self.advance();
                        let sig = self.expect_ident()?;
                        edges.push((EdgeKind::Posedge, sig));
                    } else {
                        let sig = self.expect_ident()?;
                        edges.push((EdgeKind::Posedge, sig));
                    }

                    if *self.peek() == Token::Or {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(&Token::RParen)?;
                Ok(Sensitivity::Edges(edges))
            }
            _ => Err(format!("Expected sensitivity list, got {:?}", self.peek())),
        }
    }

    fn parse_statement(&mut self) -> Result<Statement, String> {
        match self.peek().clone() {
            Token::Begin => {
                self.advance();
                let mut stmts = Vec::new();
                while *self.peek() != Token::End && *self.peek() != Token::Eof {
                    stmts.push(self.parse_statement()?);
                }
                self.expect(&Token::End)?;
                Ok(Statement::Block(stmts))
            }
            Token::If => {
                self.advance();
                self.expect(&Token::LParen)?;
                let cond = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                let then = Box::new(self.parse_statement()?);
                let else_ = if *self.peek() == Token::Else {
                    self.advance();
                    Some(Box::new(self.parse_statement()?))
                } else {
                    None
                };
                Ok(Statement::If { cond, then, else_ })
            }
            Token::Case => {
                self.advance();
                self.expect(&Token::LParen)?;
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;

                let mut arms = Vec::new();
                let mut default = None;

                while *self.peek() != Token::Endcase && *self.peek() != Token::Eof {
                    if *self.peek() == Token::Default {
                        self.advance();
                        // Optional colon
                        if *self.peek() == Token::Colon {
                            self.advance();
                        }
                        default = Some(Box::new(self.parse_statement()?));
                    } else {
                        // Parse comma-separated match values
                        let mut values = Vec::new();
                        values.push(self.parse_expr()?);
                        while *self.peek() == Token::Comma {
                            self.advance();
                            values.push(self.parse_expr()?);
                        }
                        self.expect(&Token::Colon)?;
                        let body = self.parse_statement()?;
                        arms.push((values, body));
                    }
                }
                self.expect(&Token::Endcase)?;

                Ok(Statement::Case {
                    expr,
                    arms,
                    default,
                })
            }
            _ => {
                // Assignment statement
                let lval = self.parse_lvalue()?;
                if *self.peek() == Token::Lte {
                    // Non-blocking assignment
                    self.advance();
                    let expr = self.parse_expr()?;
                    self.expect(&Token::Semi)?;
                    Ok(Statement::NonBlocking(lval, expr))
                } else {
                    // Blocking assignment
                    self.expect(&Token::Eq)?;
                    let expr = self.parse_expr()?;
                    self.expect(&Token::Semi)?;
                    Ok(Statement::Blocking(lval, expr))
                }
            }
        }
    }

    fn parse_lvalue(&mut self) -> Result<LValue, String> {
        let name = self.expect_ident()?;
        if *self.peek() == Token::LBrack {
            self.advance();
            let idx = self.parse_expr()?;
            if *self.peek() == Token::Colon {
                // Range select
                self.advance();
                let low = self.parse_const_expr(&idx)?;
                let high_val = match self.advance() {
                    Token::Number(v, _) => v as i32,
                    tok => return Err(format!("Expected number in range, got {:?}", tok)),
                };
                self.expect(&Token::RBrack)?;
                Ok(LValue::RangeSelect(name, low, high_val))
            } else {
                self.expect(&Token::RBrack)?;
                Ok(LValue::BitSelect(name, Box::new(idx)))
            }
        } else {
            Ok(LValue::Ident(name))
        }
    }

    fn parse_const_expr(&self, expr: &Expr) -> Result<i32, String> {
        match expr {
            Expr::Number(v, _) => Ok(*v as i32),
            _ => Err("Expected constant expression".to_string()),
        }
    }

    fn parse_module_inst(&mut self) -> Result<ModuleInst, String> {
        let module_name = self.expect_ident()?;
        let inst_name = self.expect_ident()?;
        self.expect(&Token::LParen)?;

        let mut connections = Vec::new();
        if *self.peek() != Token::RParen {
            connections.push(self.parse_expr()?);
            while *self.peek() == Token::Comma {
                self.advance();
                connections.push(self.parse_expr()?);
            }
        }

        self.expect(&Token::RParen)?;
        self.expect(&Token::Semi)?;

        Ok(ModuleInst {
            module_name,
            inst_name,
            connections,
        })
    }

    // Expression parsing with precedence climbing
    pub fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_log_or()?;
        if *self.peek() == Token::Question {
            self.advance();
            let then = self.parse_expr()?;
            self.expect(&Token::Colon)?;
            let else_ = self.parse_expr()?;
            expr = Expr::Ternary(Box::new(expr), Box::new(then), Box::new(else_));
        }
        Ok(expr)
    }

    fn parse_log_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_log_and()?;
        while *self.peek() == Token::PipePipe {
            self.advance();
            let right = self.parse_log_and()?;
            left = Expr::BinOp(Box::new(left), BinOp::LogOr, Box::new(right));
        }
        Ok(left)
    }

    fn parse_log_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_bit_or()?;
        while *self.peek() == Token::AmpAmp {
            self.advance();
            let right = self.parse_bit_or()?;
            left = Expr::BinOp(Box::new(left), BinOp::LogAnd, Box::new(right));
        }
        Ok(left)
    }

    fn parse_bit_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_bit_xor()?;
        while *self.peek() == Token::Pipe {
            self.advance();
            let right = self.parse_bit_xor()?;
            left = Expr::BinOp(Box::new(left), BinOp::BitOr, Box::new(right));
        }
        Ok(left)
    }

    fn parse_bit_xor(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_bit_and()?;
        while *self.peek() == Token::Caret {
            self.advance();
            let right = self.parse_bit_and()?;
            left = Expr::BinOp(Box::new(left), BinOp::BitXor, Box::new(right));
        }
        Ok(left)
    }

    fn parse_bit_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_equality()?;
        while *self.peek() == Token::Amp {
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::BinOp(Box::new(left), BinOp::BitAnd, Box::new(right));
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_relational()?;
        loop {
            match self.peek() {
                Token::EqEq => {
                    self.advance();
                    let right = self.parse_relational()?;
                    left = Expr::BinOp(Box::new(left), BinOp::Eq, Box::new(right));
                }
                Token::Neq => {
                    self.advance();
                    let right = self.parse_relational()?;
                    left = Expr::BinOp(Box::new(left), BinOp::Neq, Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_additive()?;
        loop {
            match self.peek() {
                Token::Lt => {
                    self.advance();
                    let right = self.parse_additive()?;
                    left = Expr::BinOp(Box::new(left), BinOp::Lt, Box::new(right));
                }
                Token::Gt => {
                    self.advance();
                    let right = self.parse_additive()?;
                    left = Expr::BinOp(Box::new(left), BinOp::Gt, Box::new(right));
                }
                Token::Lte => {
                    self.advance();
                    let right = self.parse_additive()?;
                    left = Expr::BinOp(Box::new(left), BinOp::Lte, Box::new(right));
                }
                Token::Gte => {
                    self.advance();
                    let right = self.parse_additive()?;
                    left = Expr::BinOp(Box::new(left), BinOp::Gte, Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        loop {
            match self.peek() {
                Token::Plus => {
                    self.advance();
                    let right = self.parse_unary()?;
                    left = Expr::BinOp(Box::new(left), BinOp::Add, Box::new(right));
                }
                Token::Minus => {
                    self.advance();
                    let right = self.parse_unary()?;
                    left = Expr::BinOp(Box::new(left), BinOp::Sub, Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Token::Tilde => {
                self.advance();
                let expr = self.parse_primary()?;
                Ok(Expr::UnaryOp(UnaryOp::BitNot, Box::new(expr)))
            }
            Token::Bang => {
                self.advance();
                let expr = self.parse_primary()?;
                Ok(Expr::UnaryOp(UnaryOp::LogNot, Box::new(expr)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            Token::Number(val, width) => {
                self.advance();
                Ok(Expr::Number(val, width))
            }
            Token::Ident(name) => {
                self.advance();
                // Check for bit select
                if *self.peek() == Token::LBrack {
                    self.advance();
                    let idx = self.parse_expr()?;
                    // Check for range select
                    if *self.peek() == Token::Colon {
                        self.advance();
                        let _low = self.parse_expr()?;
                        self.expect(&Token::RBrack)?;
                        // For range select in expression, just return the base ident
                        // since the port type already handles the width
                        return Ok(Expr::Ident(name));
                    }
                    self.expect(&Token::RBrack)?;
                    Ok(Expr::BitSelect(
                        Box::new(Expr::Ident(name)),
                        Box::new(idx),
                    ))
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            _ => Err(format!("Expected expression, got {:?} at pos {}", self.peek(), self.pos)),
        }
    }
}
