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

use crate::x11::client::XcrabWindowManager;
use crate::Result;
use breadx::AsyncDisplay;
use std::path::Path;
use std::str::FromStr;
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tokio::sync::mpsc::UnboundedSender;

macro_rules! unwrap_or_continue {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(_) => continue,
        }
    };
}

// TODO: Accept some sort of handle to perform tasks on the WM
pub async fn listener_task<P: AsRef<Path>>(
    socket_path: P,
    sender: UnboundedSender<String>,
) -> Result<()> {
    let socket_path = socket_path.as_ref();
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }
    let listener = UnixListener::bind(socket_path)?;
    loop {
        let (mut stream, _) = unwrap_or_continue!(listener.accept().await);
        let mut buf = String::new();

        stream.read_to_string(&mut buf).await?;

        drop(sender.send(buf)); // go back to ms word clippy
    }
}

pub async fn on_recv<Dpy: AsyncDisplay + ?Sized>(
    data: String,
    manager: &mut XcrabWindowManager,
    conn: &mut Dpy,
) -> Result<()> {
    let a: Action = data.parse()?;
    a.eval(manager, conn).await?;

    Ok(())
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Action {
    Close,
}

impl FromStr for Action {
    // TODO: why
    // there are conventions for this you know, like making it `impl Error`!!!
    // thats why its *not* recommended to use () if there is no meaningful error data!
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        #[allow(clippy::enum_glob_use)]
        use Action::*;

        macro_rules! eq_ignore_ascii_case_match {
            (($scrutinee:expr) { $($s:literal => $v:expr,)+ else => $else:expr $(,)? }) => {
                $(
                    if $scrutinee.eq_ignore_ascii_case($s) {
                        $v
                    } else
                )+ {
                    $else
                }
            };
        }

        eq_ignore_ascii_case_match!((s) {
            "close" => Ok(Close),
            else => Err(format!("Unknown action: {}", s)),
        })
    }
}

impl Action {
    pub async fn eval<Dpy: AsyncDisplay + ?Sized>(
        &self,
        manager: &mut XcrabWindowManager,
        conn: &mut Dpy,
    ) -> Result<()> {
        #[allow(clippy::enum_glob_use)]
        use Action::*;

        match self {
            Close => manager.destroy_focused_client(conn).await?,
        }

        Ok(())
    }
}
