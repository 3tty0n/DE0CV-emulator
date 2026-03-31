#[derive(Debug, Clone)]
pub struct VerilogModule {
    pub name: String,
    pub port_names: Vec<String>,
    pub items: Vec<ModuleItem>,
}

#[derive(Debug, Clone)]
pub enum ModuleItem {
    PortDecl(PortDecl),
    RegDecl(RegDecl),
    WireDecl(WireDecl),
    Assign(Assign),
    Always(AlwaysBlock),
    ModuleInst(ModuleInst),
}

#[derive(Debug, Clone)]
pub struct PortDecl {
    pub dir: PortDir,
    pub kind: Option<SignalKind>,
    pub range: Option<(i32, i32)>,
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PortDir {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SignalKind {
    Reg,
    Wire,
}

#[derive(Debug, Clone)]
pub struct RegDecl {
    pub range: Option<(i32, i32)>,
    pub items: Vec<(String, Option<Expr>)>,
}

#[derive(Debug, Clone)]
pub struct WireDecl {
    pub range: Option<(i32, i32)>,
    pub items: Vec<(String, Option<Expr>)>,
}

#[derive(Debug, Clone)]
pub struct Assign {
    pub target: LValue,
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub struct AlwaysBlock {
    pub sensitivity: Sensitivity,
    pub body: Statement,
}

#[derive(Debug, Clone)]
pub enum Sensitivity {
    Star,
    Edges(Vec<(EdgeKind, String)>),
}

#[derive(Debug, Clone, Copy)]
pub enum EdgeKind {
    Posedge,
}

#[derive(Debug, Clone)]
pub struct ModuleInst {
    pub module_name: String,
    pub inst_name: String,
    pub connections: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Block(Vec<Statement>),
    If {
        cond: Expr,
        then: Box<Statement>,
        else_: Option<Box<Statement>>,
    },
    Case {
        expr: Expr,
        arms: Vec<(Vec<Expr>, Statement)>,
        default: Option<Box<Statement>>,
    },
    Blocking(LValue, Expr),
    NonBlocking(LValue, Expr),
}

#[derive(Debug, Clone)]
pub enum LValue {
    Ident(String),
    BitSelect(String, Box<Expr>),
    RangeSelect(String, i32, i32),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Number(u64, Option<u32>),
    Ident(String),
    BitSelect(Box<Expr>, Box<Expr>),
    BinOp(Box<Expr>, BinOp, Box<Expr>),
    UnaryOp(UnaryOp, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Eq,
    Neq,
    Gte,
    Lte,
    Gt,
    Lt,
    BitAnd,
    BitOr,
    BitXor,
    LogAnd,
    LogOr,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    BitNot,
    LogNot,
}
