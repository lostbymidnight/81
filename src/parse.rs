use std::collections::HashMap;
use std::fmt;

pub const MODULUS: u64 = 150_094_635_296_999_121;
pub const MAX_VALUE: u64 = MODULUS - 1;

const DIGITS: &[u8; 81] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!@#$%^&*+/|\\<>~`?=_";

const MAX_LITERAL_DIGITS: usize = 9;
const MAX_ADDRESS_DIGITS: usize = 4;

#[derive(Debug)]
pub struct Error {
    pub line: usize,
    pub msg: String,
}

impl Error {
    pub fn new(line: usize, msg: impl Into<String>) -> Self {
        Error {
            line,
            msg: msg.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reg {
    A,
    X,
    Y,
    Z,
    V,
}

impl Reg {
    pub fn index(self) -> usize {
        self as usize
    }

    fn from_name(name: &str) -> Option<Reg> {
        match name {
            "A" => Some(Reg::A),
            "RX" => Some(Reg::X),
            "RY" => Some(Reg::Y),
            "RZ" => Some(Reg::Z),
            "V" => Some(Reg::V),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand {
    Lit(u64),
    Addr(u32),
    Reg(Reg),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opcode {
    Cpy,
    Inc,
    Dec,
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Clr,
    Swp,
    Eql,
    Neq,
    Grt,
    Lss,
    Jmp,
    Jeq,
    Jnq,
    Jgr,
    Jls,
    Jcd,
    Ret,
    Out,
    Inp,
    Nou,
    Nin,
    Hlt,
    Nop,
}

impl Opcode {
    fn lookup(name: &str) -> Option<(Opcode, usize, bool)> {
        use Opcode::*;
        Some(match name {
            "CPY" => (Cpy, 2, false),
            "INC" => (Inc, 1, false),
            "DEC" => (Dec, 1, false),
            "ADD" => (Add, 2, false),
            "SUB" => (Sub, 2, false),
            "MUL" => (Mul, 2, false),
            "DIV" => (Div, 2, false),
            "POW" => (Pow, 2, false),
            "CLR" => (Clr, 1, false),
            "SWP" => (Swp, 2, false),
            "EQL" => (Eql, 2, false),
            "NEQ" => (Neq, 2, false),
            "GRT" => (Grt, 2, false),
            "LSS" => (Lss, 2, false),
            "JMP" => (Jmp, 0, true),
            "JEQ" => (Jeq, 2, true),
            "JNQ" => (Jnq, 2, true),
            "JGR" => (Jgr, 2, true),
            "JLS" => (Jls, 2, true),
            "JCD" => (Jcd, 1, true),
            "RET" => (Ret, 0, false),
            "OUT" => (Out, 1, false),
            "INP" => (Inp, 0, false),
            "NOU" => (Nou, 1, false),
            "NIN" => (Nin, 0, false),
            "HLT" => (Hlt, 0, false),
            "NOP" => (Nop, 0, false),
            _ => return None,
        })
    }

    fn destinations(self) -> &'static [usize] {
        match self {
            Opcode::Cpy => &[1],
            Opcode::Inc | Opcode::Dec | Opcode::Clr => &[0],
            Opcode::Swp => &[0, 1],
            _ => &[],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Instr {
    pub op: Opcode,
    pub args: Vec<Operand>,
    pub label: Option<String>,
    pub line: usize,
}

#[derive(Debug)]
pub struct Program {
    pub instrs: Vec<Instr>,
    pub labels: HashMap<String, usize>,
}

fn parse_base81(text: &str, max_digits: usize) -> Result<u64, String> {
    if text.is_empty() {
        return Err("empty number".into());
    }
    if text.chars().count() > max_digits {
        return Err(format!("`{text}` has more than {max_digits} digits"));
    }
    let mut n = 0u64;
    for ch in text.chars() {
        let digit = DIGITS
            .iter()
            .position(|&d| d as char == ch)
            .ok_or_else(|| format!("`{ch}` is not a base-81 digit"))?;
        n = n * 81 + digit as u64;
    }
    Ok(n)
}

fn parse_operand(token: &str) -> Result<Operand, String> {
    if let Some(inner) = token.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        return parse_base81(inner, MAX_LITERAL_DIGITS).map(Operand::Lit);
    }
    if let Some(inner) = token.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
        return parse_base81(inner, MAX_ADDRESS_DIGITS).map(|a| Operand::Addr(a as u32));
    }
    if let Some(reg) = Reg::from_name(token) {
        return Ok(Operand::Reg(reg));
    }

    parse_base81(token, MAX_LITERAL_DIGITS)
        .map(Operand::Lit)
        .map_err(|e| format!("bad operand `{token}`: {e}"))
}

fn strip_comment(line: &str) -> &str {
    let mut closing: Option<char> = None;
    for (i, c) in line.char_indices() {
        match (closing, c) {
            (Some(close), _) if c == close => closing = None,
            (Some(_), _) => {}
            (None, '[') => closing = Some(']'),
            (None, '{') => closing = Some('}'),
            (None, '@') => return &line[..i],
            _ => {}
        }
    }
    line
}

fn parse_instr(code: &str, line: usize) -> Result<Instr, Error> {
    let tokens: Vec<&str> = code.split_whitespace().collect();
    let name = tokens[0];
    let (op, n_operands, has_label) = Opcode::lookup(name)
        .ok_or_else(|| Error::new(line, format!("unknown command `{name}`")))?;

    let expected = n_operands + has_label as usize;
    if tokens.len() - 1 != expected {
        return Err(Error::new(
            line,
            format!(
                "{name} takes {expected} argument(s), found {}",
                tokens.len() - 1
            ),
        ));
    }

    let args = tokens[1..=n_operands]
        .iter()
        .map(|t| parse_operand(t))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Error::new(line, e))?;

    for &i in op.destinations() {
        if matches!(args[i], Operand::Lit(_)) {
            return Err(Error::new(
                line,
                format!(
                    "{name}: argument {} must be a memory address or register",
                    i + 1
                ),
            ));
        }
    }

    let label = has_label.then(|| tokens[tokens.len() - 1].to_string());
    Ok(Instr {
        op,
        args,
        label,
        line,
    })
}

pub fn parse(src: &str) -> Result<Program, Error> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let mut instrs = Vec::new();
    let mut labels = HashMap::new();

    for (i, raw) in src.lines().enumerate() {
        let line = i + 1;
        let trimmed = raw.trim();
        if trimmed.starts_with("===") {
            continue;
        }
        let code = strip_comment(trimmed).trim();
        if code.is_empty() {
            continue;
        }
        if let Some(rest) = code.strip_prefix(')') {
            let name = rest.trim();
            if name.is_empty() || name.contains(char::is_whitespace) {
                return Err(Error::new(line, "a label needs exactly one name"));
            }
            if labels.insert(name.to_string(), instrs.len()).is_some() {
                return Err(Error::new(line, format!("label `{name}` is defined twice")));
            }
            continue;
        }
        instrs.push(parse_instr(code, line)?);
    }

    Ok(Program { instrs, labels })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits() {
        assert_eq!(parse_base81("0", 9), Ok(0));
        assert_eq!(parse_base81("A", 9), Ok(10));
        assert_eq!(parse_base81("Z", 9), Ok(35));
        assert_eq!(parse_base81("a", 9), Ok(36));
        assert_eq!(parse_base81("z", 9), Ok(61));
        assert_eq!(parse_base81("!", 9), Ok(62));
        assert_eq!(parse_base81("@", 9), Ok(63));
        assert_eq!(parse_base81("*", 9), Ok(69));
        assert_eq!(parse_base81("+", 9), Ok(70));
        assert_eq!(parse_base81("/", 9), Ok(71));
        assert_eq!(parse_base81("|", 9), Ok(72));
        assert_eq!(parse_base81("=", 9), Ok(79));
        assert_eq!(parse_base81("_", 9), Ok(80));
        assert_eq!(parse_base81("10", 9), Ok(81));
        assert_eq!(parse_base81("1K", 9), Ok(101));
    }

    #[test]
    fn limits() {
        assert_eq!(parse_base81("_________", 9), Ok(MAX_VALUE));
        assert!(parse_base81("__________", 9).is_err());
        assert!(parse_base81("", 9).is_err());
        assert!(parse_base81("-", 9).is_err());
        assert_eq!(parse_base81("____", 4), Ok(43_046_720));
        assert!(parse_base81("_____", 4).is_err());
    }

    #[test]
    fn comments() {
        assert_eq!(strip_comment("OUT [@] @ hi"), "OUT [@] ");
        assert_eq!(strip_comment("CPY {@} A"), "CPY {@} A");
        assert_eq!(strip_comment("NOP @ x"), "NOP ");
    }

    #[test]
    fn operands() {
        assert_eq!(parse_operand("[1K]"), Ok(Operand::Lit(101)));
        assert_eq!(parse_operand("{10}"), Ok(Operand::Addr(81)));
        assert_eq!(parse_operand("RX"), Ok(Operand::Reg(Reg::X)));
        assert_eq!(parse_operand("0"), Ok(Operand::Lit(0)));
        assert!(parse_operand("[").is_err());
    }

    #[test]
    fn windows_files() {
        let p = parse("\u{feff}OUT [W]\r\n) a\r\nNOP\r\n").unwrap();
        assert_eq!(p.instrs.len(), 2);
        assert_eq!(p.labels["a"], 1);
    }

    #[test]
    fn labels() {
        let p = parse("=== top ===\nNOP\n) here\n  NOP @ c\n").unwrap();
        assert_eq!(p.instrs.len(), 2);
        assert_eq!(p.labels["here"], 1);
    }

    #[test]
    fn bad_input() {
        assert!(parse("FOO").is_err());
        assert!(parse("ADD [1]").is_err());
        assert!(parse("INC [1]").is_err());
        assert!(parse(") a\n) a").is_err());
    }
}
