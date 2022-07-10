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

use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    let msg = std::env::args().skip(1).collect::<Vec<String>>().join(" ");

    let conf = config::load_file();

    let path = conf.msg.socket_path;

    let mut stream = UnixStream::connect(path).await?;

    stream.write_all(msg.as_bytes()).await?;

    Ok(())
}
