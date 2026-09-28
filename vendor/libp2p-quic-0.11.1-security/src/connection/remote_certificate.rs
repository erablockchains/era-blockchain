// Copyright 2020 Parity Technologies (UK) Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// MIT license; see ../connecting.rs and ../../LICENSE.
// Isolated certificate-to-PeerId validation used after the TLS handshake.
use libp2p_identity::PeerId;
use quinn::rustls::pki_types::CertificateDer;
use crate::Error;

pub(crate) fn remote_peer_id(certificates: &[CertificateDer<'_>]) -> Result<PeerId, Error> {
    let end_entity = certificates.first().ok_or_else(|| invalid_certificate("No certificate found"))?;
    let p2p_cert = libp2p_tls::certificate::parse(end_entity)
        .map_err(|_| invalid_certificate("Could not parse certificate"))?;
    Ok(p2p_cert.peer_id())
}

fn invalid_certificate(message: &'static str) -> Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_certificate_chain_returns_error() {
        assert!(remote_peer_id(&[]).is_err());
    }
    #[test]
    fn malformed_certificate_returns_error() {
        assert!(remote_peer_id(&[CertificateDer::from(vec![0, 1, 2])]).is_err());
    }
    #[test]
    fn valid_libp2p_certificate_preserves_peer_id() {
        let key = libp2p_identity::Keypair::generate_ed25519();
        let (cert, _) = libp2p_tls::certificate::generate(&key).unwrap();
        assert_eq!(remote_peer_id(&[cert]).unwrap(), key.public().to_peer_id());
    }
}
