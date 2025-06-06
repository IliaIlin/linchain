use crate::blockchain::PublicKey;
use k256::ecdsa::signature::{Signer, Verifier};
use k256::ecdsa::{Signature, SigningKey, VerifyingKey};
use std::error::Error;

pub fn sign_ecdsa<S: serde::Serialize>(
    data: &S,
    signing_key: &SigningKey,
) -> Result<String, Box<dyn Error>> {
    let serialized = bcs::to_bytes(&data)?;
    let signature: Signature = signing_key.sign(&serialized);
    Ok(hex::encode(signature.to_bytes()))
}

pub fn verify_ecdsa<S: serde::Serialize>(
    data: &S,
    signature_hex: &str,
    public_key: &PublicKey,
) -> Result<bool, Box<dyn Error>> {
    let verifying_key = VerifyingKey::from_sec1_bytes(public_key)?;
    let serialized = bcs::to_bytes(&data)?;
    let signature_bytes = hex::decode(signature_hex)?;
    let signature = Signature::from_slice(&signature_bytes)?;
    match verifying_key.verify(&serialized, &signature) {
        Ok(_) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use crate::blockchain::PublicKey;
    use crate::crypto::{sign_ecdsa, verify_ecdsa};
    use k256::ecdsa::SigningKey;
    use k256::elliptic_curve::rand_core::OsRng;
    use serde_derive::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    struct TestData {
        message: String,
        number: u32,
    }

    #[test]
    fn test_sign_verify_happy_flow() {
        let test_data = TestData {
            message: "testTest".to_string(),
            number: 42,
        };

        let signing_key = SigningKey::random(&mut OsRng);

        let verifying_key = signing_key.verifying_key();
        let public_key_bytes: PublicKey = verifying_key.to_sec1_bytes().to_vec();

        let signature = sign_ecdsa(&test_data, &signing_key).unwrap();

        let result = verify_ecdsa(&test_data, &signature, &public_key_bytes);
        result.expect("Signature verification should succeed.");
    }
}
