// PRSS retursn in p7b format. This converts to PEM format.
use bytes::Bytes;
use cms::cert::CertificateChoices;
use cms::cert::x509::certificate::CertificateInner;
use cms::cert::x509::der::{Decode, Encode};
use cms::{content_info::ContentInfo, signed_data::SignedData};
use std::collections::HashMap;

pub struct CertificateChain(Bytes);

impl CertificateChain {
    pub fn from_cert_chain(cert_chain: Bytes) -> Self {
        Self(cert_chain)
    }

    fn sort_certificates(certs: Vec<&CertificateInner>) -> c2pa::Result<Vec<Vec<u8>>> {
        if certs.is_empty() {
            return Err(c2pa::Error::CoseX5ChainMissing);
        }

        let max_list_length = certs.len();
        let mut list = Vec::with_capacity(max_list_length);

        if certs.len() == 1 {
            list.push(certs.first().unwrap());
        } else {
            let mut subject_of_cert_map = HashMap::new();
            let mut issuer_of_cert_map = HashMap::new();

            // Map subjects and issuers of the certificates for quick lookup
            for cert in certs.iter() {
                let subject = cert.tbs_certificate.subject.to_string();
                let issuer = cert.tbs_certificate.issuer.to_string();

                // Do not include the root CA.
                // https://c2pa.org/specifications/specifications/2.0/specs/C2PA_Specification.html#x509_certificates
                if subject != issuer {
                    issuer_of_cert_map.insert(issuer, cert);
                } else {
                    list.push(cert);
                }

                subject_of_cert_map.insert(subject, cert);
            }

            // Find the top most cert
            if list.is_empty() {
                // For each issuer, look to see if it is found as a subject of certificate. If it is not found it is the top most cert
                if let Some((_, &cert)) = issuer_of_cert_map
                    .iter()
                    .find(|(issuer, _)| !subject_of_cert_map.contains_key(issuer.as_str()))
                {
                    list.push(cert);
                }
            }

            // Build out the certificate chain from root to leaf
            for _i in 1..max_list_length {
                let last = list.last().unwrap();
                let subject = last.tbs_certificate.subject.to_string();

                if let Some(&cert) = issuer_of_cert_map.get(&subject) {
                    list.push(cert);
                } else {
                    return Err(c2pa::Error::CoseInvalidCert);
                }
            }
        }

        // Reverse iterate and convert the certifcates
        list.into_iter()
            .rev()
            .inspect(|c| {
                log::debug!(
                    "cert: Subject= ({}) Issuer= ({})",
                    c.tbs_certificate.subject,
                    c.tbs_certificate.issuer
                )
            })
            .map(|c| c.to_der().map_err(|_| c2pa::Error::CoseInvalidCert))
            .collect()
    }

    pub fn get_pem_certificates(&self) -> c2pa::Result<Vec<Vec<u8>>> {
        let info = ContentInfo::from_der(&self.0)
            .inspect_err(|x| log::error!("{:?}", x))
            .map_err(|_| c2pa::Error::CoseInvalidCert)?;
        let data: SignedData = info
            .content
            .decode_as()
            .map_err(|_| c2pa::Error::CoseInvalidCert)?;
        if let Some(certs) = data.certificates {
            let certs: Vec<_> = certs
                .0
                .iter()
                .filter_map(|c| match c {
                    CertificateChoices::Certificate(c) => Some(c),
                    _ => None,
                })
                .collect();
            return Self::sort_certificates(certs);
        }
        Err(c2pa::Error::CoseX5ChainMissing)?
    }
}
