// Copyright (C) 2022 Infoshock Tech

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

#![warn(clippy::pedantic)]

mod config;

use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct CustomError(String);

impl Debug for CustomError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(&self.0)
    }
}

impl Display for CustomError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(&self.0)
    }
}

impl Error for CustomError {}

#[tokio::main]
async fn main() -> Result<()> {
    let msg = std::env::args().skip(1).collect::<Vec<String>>().join(" ");

    let conf = config::load_file();

    let path = conf.msg.socket_path;

    let stream = UnixStream::connect(path).await?;

    let (mut read, mut write) = stream.into_split();

    write.write_all(msg.as_bytes()).await?;
    drop(write); // Shutdown the writer half so that the write actually goes through
                 // "Don't cross the streams!""

    let mut buf = String::new();

    read.read_to_string(&mut buf).await?;
    if !buf.is_empty() {
        return Err(CustomError(buf).into());
    }

    Ok(())
}
