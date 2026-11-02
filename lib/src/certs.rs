//! Loading and ordering of PEM certificate chains for KMS-backed signers.
//!
//! Cloud key management services (AWS KMS, Google Cloud KMS) only hold the
//! private key, so the signing certificate chain issued by your C2PA CA has to
//! be supplied separately.

use std::collections::{HashMap, HashSet};

use cms::cert::x509::{Certificate, der::Encode};

/// Loads a PEM certificate chain and returns it in the order C2PA expects.
///
/// `source` is either inline PEM text (starting with `-----BEGIN`) or a path
/// to a PEM file. See [`certificate_chain_from_pem`] for the ordering rules.
pub fn load_certificate_chain(source: &str) -> c2pa::Result<Vec<Vec<u8>>> {
    let trimmed = source.trim_start();
    if trimmed.starts_with("-----BEGIN") {
        certificate_chain_from_pem(trimmed.as_bytes())
    } else {
        certificate_chain_from_pem(&std::fs::read(source)?)
    }
}

/// Parses PEM certificates and returns DER-encoded certificates ordered from
/// the signing (leaf) certificate up to the last intermediate.
///
/// The certificates may appear in any order. Self-signed root certificates are
/// dropped because trust anchors must not be embedded in the C2PA `x5chain`,
/// unless the chain contains nothing but a single self-signed certificate.
pub fn certificate_chain_from_pem(pem: &[u8]) -> c2pa::Result<Vec<Vec<u8>>> {
    // x509-cert's `load_pem_chain` panics on empty input.
    if pem.iter().all(u8::is_ascii_whitespace) {
        return Err(c2pa::Error::CoseX5ChainMissing);
    }
    let certificates = Certificate::load_pem_chain(pem).map_err(|err| {
        log::error!("Failed to parse PEM certificate chain: {err}");
        c2pa::Error::CoseInvalidCert
    })?;
    order_chain(&certificates)?
        .into_iter()
        .map(|cert| cert.to_der().map_err(|_| c2pa::Error::CoseInvalidCert))
        .collect()
}

fn subject(cert: &Certificate) -> String {
    cert.tbs_certificate.subject.to_string()
}

fn issuer(cert: &Certificate) -> String {
    cert.tbs_certificate.issuer.to_string()
}

fn is_self_signed(cert: &Certificate) -> bool {
    cert.tbs_certificate.subject == cert.tbs_certificate.issuer
}

fn order_chain(certificates: &[Certificate]) -> c2pa::Result<Vec<&Certificate>> {
    match certificates {
        [] => return Err(c2pa::Error::CoseX5ChainMissing),
        [only] => return Ok(vec![only]),
        _ => {}
    }

    let issuers: HashSet<String> = certificates
        .iter()
        .filter(|cert| !is_self_signed(cert))
        .map(issuer)
        .collect();
    let by_subject: HashMap<String, &Certificate> = certificates
        .iter()
        .map(|cert| (subject(cert), cert))
        .collect();

    // The leaf is the only non-root certificate that did not issue another one.
    let mut leaves = certificates
        .iter()
        .filter(|cert| !is_self_signed(cert) && !issuers.contains(&subject(cert)));
    let leaf = leaves.next().ok_or(c2pa::Error::CoseInvalidCert)?;
    if leaves.next().is_some() {
        log::error!("Certificate chain contains more than one leaf certificate");
        return Err(c2pa::Error::CoseInvalidCert);
    }

    let mut chain = vec![leaf];
    let mut current = leaf;
    while chain.len() < certificates.len() {
        match by_subject.get(&issuer(current)) {
            Some(next) if !is_self_signed(next) => {
                chain.push(next);
                current = next;
            }
            _ => break,
        }
    }

    for cert in &chain {
        log::debug!("cert: Subject= ({}) Issuer= ({})", subject(cert), issuer(cert));
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cms::cert::x509::der::Decode;

    const UNORDERED_CHAIN: &str = include_str!("../../test_data/certs/unordered_chain.pem");

    fn common_names(chain: &[Vec<u8>]) -> Vec<String> {
        chain
            .iter()
            .map(|der| subject(&Certificate::from_der(der).unwrap()))
            .collect()
    }

    #[test]
    fn orders_leaf_first_and_drops_root() {
        let chain = certificate_chain_from_pem(UNORDERED_CHAIN.as_bytes()).unwrap();
        assert_eq!(
            common_names(&chain),
            [
                "CN=c2pa-cloud test signer",
                "CN=c2pa-cloud test intermediate"
            ]
        );
    }

    #[test]
    fn loads_inline_pem_and_files() {
        let inline = load_certificate_chain(UNORDERED_CHAIN).unwrap();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../test_data/certs/unordered_chain.pem"
        );
        let from_file = load_certificate_chain(path).unwrap();
        assert_eq!(inline, from_file);
    }

    #[test]
    fn keeps_single_self_signed_certificate() {
        let root = UNORDERED_CHAIN
            .split_inclusive("-----END CERTIFICATE-----")
            .next()
            .unwrap();
        let chain = certificate_chain_from_pem(root.as_bytes()).unwrap();
        assert_eq!(common_names(&chain), ["CN=c2pa-cloud test root"]);
    }

    #[test]
    fn rejects_empty_and_invalid_input() {
        assert!(certificate_chain_from_pem(b"").is_err());
        assert!(certificate_chain_from_pem(b"\n\r\n").is_err());
        assert!(
            certificate_chain_from_pem(
                b"-----BEGIN CERTIFICATE-----\nAA==\n-----END CERTIFICATE-----\n"
            )
            .is_err()
        );
        assert!(load_certificate_chain("/does/not/exist.pem").is_err());
    }
}
