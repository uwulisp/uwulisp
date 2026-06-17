use std::{cell::RefCell, io::{self, BufRead, BufReader}};

// A thread-local stdin buffer shared between the REPL and (read-line).
// Both the REPL loop and the builtin pull lines from the same BufReader,
// so they never race on the underlying fd.
thread_local! {
    pub static STDIN_BUF: RefCell<BufReader<io::Stdin>> =
        RefCell::new(BufReader::new(io::stdin()));
}

/// Read one raw line from the shared stdin buffer.
/// Returns None on EOF, Some(line) with the trailing newline stripped otherwise.
pub fn shared_read_line() -> Result<Option<String>, String> {
    STDIN_BUF.with(|buf| {
        let mut line = String::new();
        match buf.borrow_mut().read_line(&mut line) {
            Ok(0) => Ok(None), // EOF
            Ok(_) => {
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') { line.pop(); }
                }
                Ok(Some(line))
            }
            Err(e) => Err(format!("read-line: {}", e)),
        }
    })
}