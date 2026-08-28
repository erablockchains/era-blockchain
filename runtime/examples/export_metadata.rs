use codec::Encode;
use std::io::{self, Write};

fn main() -> io::Result<()> {
    io::stdout().write_all(&era_runtime::Runtime::metadata().encode())
}
