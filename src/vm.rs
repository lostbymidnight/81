use crate::parse::{Error, Instr, MAX_VALUE, MODULUS, Opcode, Operand, Program, Reg};
use std::collections::{HashMap, VecDeque};
use std::fmt::Arguments;
use std::io::{self, BufRead, Write};

type Wrapped = (u64, bool);

fn wrap(n: u128) -> Wrapped {
    ((n % MODULUS as u128) as u64, n >= MODULUS as u128)
}

fn pow(base: u64, mut exp: u64) -> Wrapped {
    let m = MODULUS as u128;

    let (mut rm, mut rc) = (1u128, 1u128);
    let (mut bm, mut bc) = (base as u128, base as u128);

    while exp > 0 {
        if exp & 1 == 1 {
            rm = rm * bm % m;
            rc = (rc * bc).min(m);
        }

        exp >>= 1;

        if exp > 0 {
            bm = bm * bm % m;
            bc = (bc * bc).min(m);
        }
    }

    (rm as u64, rc >= m)
}

pub struct Vm<R: BufRead, W: Write> {
    mem: HashMap<u32, u64>,
    regs: [u64; 5],
    stack: Vec<usize>,
    input: R,
    pending: VecDeque<char>,
    out: W,
}

impl<R: BufRead, W: Write> Vm<R, W> {
    pub fn new(input: R, out: W) -> Self {
        Vm {
            mem: HashMap::new(),
            regs: [0; 5],
            stack: Vec::new(),
            input,
            pending: VecDeque::new(),
            out,
        }
    }

    pub fn run(&mut self, prog: &Program) -> Result<(), Error> {
        let result = self.exec(prog);
        let _ = self.out.flush();
        result
    }

    fn read(&self, op: Operand) -> u64 {
        match op {
            Operand::Lit(v) => v,
            Operand::Addr(a) => self.mem.get(&a).copied().unwrap_or(0),
            Operand::Reg(r) => self.regs[r.index()],
        }
    }

    fn write(&mut self, op: Operand, value: u64) {
        match op {
            Operand::Addr(a) => {
                self.mem.insert(a, value);
            }
            Operand::Reg(r) => self.regs[r.index()] = value,
            Operand::Lit(_) => unreachable!("the parser rejects literal destinations"),
        }
    }

    fn update(&mut self, dest: Operand, (value, wrapped): Wrapped) {
        self.regs[Reg::V.index()] = 0;
        self.write(dest, value);

        if wrapped {
            self.regs[Reg::V.index()] = 1;
        }
    }

    fn jump(&mut self, prog: &Program, ins: &Instr, pc: usize) -> Result<usize, Error> {
        let label = ins.label.as_deref().unwrap();

        match prog.labels.get(label) {
            Some(&target) => {
                self.stack.push(pc + 1);
                Ok(target)
            }
            None => Err(Error::new(
                ins.line,
                format!("jump to undefined label `{label}`"),
            )),
        }
    }

    fn exec(&mut self, prog: &Program) -> Result<(), Error> {
        let a = Reg::A.index();
        let v = Reg::V.index();
        let mut pc = 0;

        while pc < prog.instrs.len() {
            let ins = &prog.instrs[pc];
            let line = ins.line;
            let arg = |n: usize| ins.args[n];
            let mut next = pc + 1;

            match ins.op {
                Opcode::Cpy => {
                    let value = self.read(arg(0));
                    self.write(arg(1), value);
                }
                Opcode::Inc => {
                    let x = self.read(arg(0));
                    let res = if x == MAX_VALUE {
                        (0, true)
                    } else {
                        (x + 1, false)
                    };

                    self.update(arg(0), res);
                }
                Opcode::Dec => {
                    let x = self.read(arg(0));
                    let res = if x == 0 {
                        (MAX_VALUE, true)
                    } else {
                        (x - 1, false)
                    };

                    self.update(arg(0), res);
                }
                Opcode::Add | Opcode::Sub | Opcode::Mul | Opcode::Div | Opcode::Pow => {
                    let (x, y) = (self.read(arg(0)), self.read(arg(1)));
                    let (value, wrapped) = match ins.op {
                        Opcode::Add => wrap(x as u128 + y as u128),
                        Opcode::Sub if x >= y => (x - y, false),
                        Opcode::Sub => (MODULUS - (y - x), true),
                        Opcode::Mul => wrap(x as u128 * y as u128),
                        Opcode::Div if y == 0 => {
                            return Err(Error::new(line, "division by zero"));
                        }
                        Opcode::Div => (x / y, false),
                        _ if x == 0 && y == 0 => {
                            return Err(Error::new(line, "0 to the power of 0 is undefined"));
                        }
                        _ => pow(x, y),
                    };

                    self.regs[a] = value;
                    self.regs[v] = wrapped as u64;
                }
                Opcode::Clr => self.write(arg(0), 0),
                Opcode::Swp => {
                    let (x, y) = (self.read(arg(0)), self.read(arg(1)));
                    self.write(arg(0), y);
                    self.write(arg(1), x);
                }
                Opcode::Eql | Opcode::Neq | Opcode::Grt | Opcode::Lss => {
                    let (x, y) = (self.read(arg(0)), self.read(arg(1)));
                    let holds = match ins.op {
                        Opcode::Eql => x == y,
                        Opcode::Neq => x != y,
                        Opcode::Grt => x > y,
                        _ => x < y,
                    };

                    self.regs[a] = holds as u64;
                }
                Opcode::Jmp
                | Opcode::Jeq
                | Opcode::Jnq
                | Opcode::Jgr
                | Opcode::Jls
                | Opcode::Jcd => {
                    let take = match ins.op {
                        Opcode::Jmp => true,
                        Opcode::Jcd => self.read(arg(0)) == 1,
                        op => {
                            let (x, y) = (self.read(arg(0)), self.read(arg(1)));
                            match op {
                                Opcode::Jeq => x == y,
                                Opcode::Jnq => x != y,
                                Opcode::Jgr => x > y,
                                _ => x < y,
                            }
                        }
                    };
                    if take {
                        next = self.jump(prog, ins, pc)?;
                    }
                }
                Opcode::Ret => {
                    next = self
                        .stack
                        .pop()
                        .ok_or_else(|| Error::new(line, "RET with an empty call stack"))?;
                }
                Opcode::Out => {
                    let code = self.read(arg(0));
                    let ch = u32::try_from(code)
                        .ok()
                        .and_then(char::from_u32)
                        .ok_or_else(|| {
                            Error::new(line, format!("{code} is not a valid Unicode code point"))
                        })?;
                    if !self.emit(format_args!("{ch}"), line)? {
                        return Ok(());
                    }
                }
                Opcode::Nou => {
                    let n = self.read(arg(0));
                    if !self.emit(format_args!("{n}"), line)? {
                        return Ok(());
                    }
                }
                Opcode::Inp => {
                    let _ = self.out.flush();
                    let c = self.peek();
                    if c.is_some() {
                        self.pending.pop_front();
                    }
                    self.regs[a] = c.map_or(0, |c| c as u64);
                }
                Opcode::Nin => {
                    let _ = self.out.flush();
                    self.regs[a] = self.read_number().map_err(|e| Error::new(line, e))?;
                }
                Opcode::Hlt => return Ok(()),
                Opcode::Nop => {}
            }
            pc = next;
        }
        Ok(())
    }

    /// Writes output. Returns `false` if the reader closed the pipe (like `81 prog | head`),
    /// which just ends the program quietly.
    fn emit(&mut self, text: Arguments, line: usize) -> Result<bool, Error> {
        match self.out.write_fmt(text) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(false),
            Err(e) => Err(Error::new(line, e.to_string())),
        }
    }

    fn peek(&mut self) -> Option<char> {
        while self.pending.is_empty() {
            let mut line = String::new();
            match self.input.read_line(&mut line) {
                Ok(0) | Err(_) => return None,
                Ok(_) => self.pending.extend(line.chars()),
            }
        }
        self.pending.front().copied()
    }

    fn read_number(&mut self) -> Result<u64, String> {
        loop {
            match self.peek() {
                None => return Err("NIN: end of input, expected a number".into()),
                Some(c) if c.is_whitespace() => {
                    self.pending.pop_front();
                }
                Some(_) => break,
            }
        }
        let mut word = String::new();
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                break;
            }
            word.push(c);
            self.pending.pop_front();
        }
        if !word.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("NIN: `{word}` is not a base-10 number"));
        }
        match word.parse::<u128>() {
            Ok(n) if n < MODULUS as u128 => Ok(n as u64),
            _ => Err(format!("NIN: `{word}` is too large for a cell")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    fn run(src: &str, input: &str) -> (Result<(), Error>, String) {
        let prog = parse(src).unwrap();
        let mut out = Vec::new();
        let result = Vm::new(input.as_bytes(), &mut out).run(&prog);
        (result, String::from_utf8(out).unwrap())
    }

    fn ok(src: &str, input: &str) -> String {
        let (result, out) = run(src, input);
        result.unwrap();
        out
    }

    fn err(src: &str, input: &str) -> String {
        run(src, input).0.unwrap_err().msg
    }

    #[test]
    fn arithmetic() {
        assert_eq!(ok("ADD [1] [2]\nNOU A", ""), "3");
        assert_eq!(ok("SUB [9] [4]\nNOU A", ""), "5");
        assert_eq!(ok("MUL [9] [4]\nNOU A", ""), "36");
        assert_eq!(ok("DIV [9] [4]\nNOU A", ""), "2");
        assert_eq!(ok("POW [2] [A]\nNOU A", ""), "1024");
        assert_eq!(ok("POW [5] [0]\nNOU A", ""), "1");
    }

    #[test]
    fn overflow() {
        assert_eq!(ok("ADD [_________] [1]\nNOU A\nOUT [W]\nNOU V", ""), "0 1");
        assert_eq!(ok("MUL [_________] [_________]\nNOU V", ""), "1");
        assert_eq!(ok("POW [2] [1J]\nNOU V", ""), "1");
        assert_eq!(ok("POW [2] [A]\nNOU V", ""), "0");
        assert_eq!(ok("POW [0] [5]\nNOU A\nNOU V", ""), "00");
    }

    #[test]
    fn underflow() {
        assert_eq!(
            ok("SUB [3] [5]\nNOU A\nOUT [W]\nNOU V", ""),
            "150094635296999119 1"
        );
        assert_eq!(
            ok("DEC {0}\nNOU {0}\nOUT [W]\nNOU V", ""),
            "150094635296999120 1"
        );
        assert_eq!(ok("CPY [_________] {0}\nINC {0}\nNOU {0}\nNOU V", ""), "01");
    }

    #[test]
    fn v_clears() {
        let src = "ADD [_________] [1]\nADD [1] [1]\nJCD V yes\nOUT [?]\nHLT\n) yes\nOUT [18]";
        assert_eq!(ok(src, ""), "N");
        assert_eq!(ok("ADD [_________] [1]\nINC RX\nNOU V", ""), "0");
        assert_eq!(ok("ADD [_________] [1]\nEQL [1] [1]\nNOU V", ""), "1");
    }

    #[test]
    fn math_errors() {
        assert!(err("DIV [1] [0]", "").contains("division by zero"));
        assert!(err("POW [0] [0]", "").contains("undefined"));
    }

    #[test]
    fn comparisons() {
        assert_eq!(ok("EQL [1] [1]\nNOU A\nNEQ [1] [1]\nNOU A", ""), "10");
        assert_eq!(ok("GRT [2] [1]\nNOU A\nLSS [2] [1]\nNOU A", ""), "10");
    }

    #[test]
    fn memory() {
        assert_eq!(ok("CPY [5] {1}\nCPY {1} RX\nNOU RX", ""), "5");
        assert_eq!(
            ok(
                "CPY [1] {0}\nCPY [2] {1}\nSWP {0} {1}\nNOU {0}\nNOU {1}",
                ""
            ),
            "21"
        );
        assert_eq!(ok("CPY [7] {0}\nCLR {0}\nNOU {0}", ""), "0");
        assert_eq!(ok("CPY [1] V\nNOU V\nCPY [0] V\nNOU V", ""), "10");
    }

    #[test]
    fn jumps() {
        assert_eq!(ok("JMP end\nOUT [W]\n) end\nOUT [X]", ""), "!");
        assert_eq!(ok("JEQ [1] [1] t\nOUT [W]\n) t\nNOP", ""), "");
        assert_eq!(ok("JEQ [1] [2] t\nOUT [W]\n) t\nNOP", ""), " ");
        assert_eq!(ok("JGR 5 3 t\nOUT [W]\n) t\nNOP", ""), "");
        assert_eq!(ok("JLS 5 3 t\nOUT [W]\n) t\nNOP", ""), " ");
        assert_eq!(ok("JNQ 5 3 t\nOUT [W]\n) t\nNOP", ""), "");
        assert_eq!(ok("JCD [1] t\nOUT [W]\n) t\nNOP", ""), "");
        assert_eq!(ok("JCD [2] t\nOUT [W]\n) t\nNOP", ""), " ");
    }

    #[test]
    fn ret() {
        let src = "JMP a\nOUT [1]\nHLT\n) a\nJMP b\nOUT [2]\nRET\n) b\nOUT [3]\nRET";
        assert_eq!(ok(src, ""), "\u{3}\u{2}\u{1}");
        assert!(err("RET", "").contains("empty call stack"));
        assert!(err("JEQ [1] [2] x\nRET\n) x", "").contains("empty call stack"));
    }

    #[test]
    fn bad_label() {
        assert!(err("JMP nowhere", "").contains("undefined label"));
        assert_eq!(ok("JEQ [1] [2] nowhere", ""), "");
    }

    #[test]
    fn io() {
        assert_eq!(ok("INP\nOUT A\nINP\nOUT A", "hi"), "hi");
        assert_eq!(ok("INP\nNOU A", ""), "0");
        assert_eq!(ok("NIN\nNOU A", "  42\n"), "42");
        assert_eq!(ok("NIN\nCPY A RX\nNIN\nADD RX A\nNOU A", "40 2"), "42");
        assert!(err("NIN", "").contains("end of input"));
        assert!(err("NIN", "abc").contains("not a base-10"));
        assert!(err("NIN", "-5").contains("not a base-10"));
        assert!(err("NIN", "999999999999999999999").contains("too large"));
        assert!(err("OUT [_________]", "").contains("code point"));
    }

    #[test]
    fn at_literal() {
        assert_eq!(ok("OUT [@] @ comment\nOUT [0] @ A", ""), "?\u{0}");
    }

    #[test]
    fn end() {
        assert_eq!(ok("OUT [W]", ""), " ");
        assert_eq!(ok("HLT\nOUT [W]", ""), "");
    }
}
