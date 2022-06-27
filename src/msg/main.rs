mod config;

use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    let conf = config::load_file()?;

    let path = conf.msg.expect("xcrab-msg not configured!").socket_path;

    let mut stream = UnixStream::connect(path).await?;

    stream.write_all("hello world".as_bytes()).await?;

    Ok(())
}
