use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

fn spawn(args: &[&str], stdin: &str) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_81"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    Run {
        stdout: String::from_utf8(out.stdout).unwrap(),
        stderr: String::from_utf8(out.stderr).unwrap(),
        code: out.status.code().unwrap(),
    }
}

fn run(file: &str, stdin: &str) -> Run {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(file);
    spawn(&[path.to_str().unwrap()], stdin)
}

#[test]
fn hello() {
    let r = run("examples/hello.81", "");
    assert_eq!(r.stdout, "Hello, World!");
    assert_eq!(r.code, 0);
}

#[test]
fn fib() {
    assert_eq!(run("examples/fibonacci.81", "0").stdout, "");
    assert_eq!(run("examples/fibonacci.81", "1").stdout, "0");
    assert_eq!(run("examples/fibonacci.81", "2").stdout, "0 1");
    assert_eq!(run("examples/fibonacci.81", "3").stdout, "0 1 1");
    assert_eq!(
        run("examples/fibonacci.81", "10").stdout,
        "0 1 1 2 3 5 8 13 21 34"
    );
}

#[test]
fn fib_overflow() {
    let r = run("examples/fibonacci.81", "100");
    assert!(r.stdout.ends_with("99194853094755497Err"), "{}", r.stdout);
    assert!(r.stdout.starts_with("0 1 1 2 3 5"));
    assert_eq!(r.code, 0);
}

#[test]
fn prime() {
    let check = |n: &str| run("examples/primality.81", n).stdout;
    for n in ["0", "1"] {
        assert_eq!(check(n), "N/A", "{n}");
    }
    for n in ["2", "3", "5", "7", "11", "13", "97", "7919"] {
        assert_eq!(check(n), "T", "{n}");
    }
    for n in ["4", "6", "9", "15", "25", "100", "7917"] {
        assert_eq!(check(n), "F", "{n}");
    }
}

#[test]
fn error_exit() {
    let r = run("tests/programs/div_zero.81", "");
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "error: line 2: division by zero\n");
}

#[test]
fn code_flag() {
    let r = spawn(&["-c", "OUT [|]\nOUT [1O]"], "");
    assert_eq!(r.stdout, "Hi");
    assert_eq!(r.code, 0);
    let r = spawn(&["-c", "NIN\nNOU A"], "41");
    assert_eq!(r.stdout, "41");
}

#[test]
fn code_flag_errors() {
    let r = spawn(&["-c", "FOO"], "");
    assert_eq!(r.code, 1);
    assert_eq!(r.stderr, "error: line 1: unknown command `FOO`\n");
    assert_eq!(spawn(&["-c"], "").code, 2);
    assert_eq!(spawn(&[], "").code, 2);
}

#[test]
fn help() {
    for flag in ["-h", "--help"] {
        let r = spawn(&[flag], "");
        assert_eq!(r.code, 0);
        assert!(r.stdout.contains("usage:") && r.stdout.contains("81 -c <code>"));
    }
    let r = spawn(&[], "");
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("usage:"));
}

#[test]
fn closed_pipe_exits_quietly() {
    use std::io::Read;
    use std::time::{Duration, Instant};

    let mut child = Command::new(env!("CARGO_BIN_EXE_81"))
        .args(["-c", ") a\nOUT [W]\nJMP a"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = child.stdout.take().unwrap();
    out.read_exact(&mut [0u8; 1]).unwrap();
    drop(out);

    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("the program did not stop after its output pipe closed");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(status.success() && stderr.is_empty(), "{status:?} {stderr}");
}
