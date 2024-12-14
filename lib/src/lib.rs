mod acs;
mod auth;
mod p7b;
mod sign;

use std::io::{Read, Seek};

pub use c2pa::Error;
use c2pa::Reader;
pub use envconfig::Envconfig;
pub use sign::{SigningOptions, TrustedSigner};

pub async fn verify_file(
    format: &str,
    stream: impl Read + Seek + Send,
) -> Result<String, c2pa::Error> {
    let reader = Reader::from_stream_async(format, stream).await?;
    Ok(reader.json())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[tokio::test]
    async fn test_verify_file() {
        let data = include_bytes!("../../test_data/signed.png");
        let stream = Cursor::new(data);
        let result = verify_file("png", stream).await.unwrap();
        assert_eq!(&result, include_str!("../../test_data/manifest.json"));
    }
}
