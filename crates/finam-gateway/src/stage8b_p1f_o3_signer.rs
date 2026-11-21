//! Bounded O3 AF_UNIX schedule-signing adapter. The wire carries the complete
//! unsigned schedule, not an arbitrary hash. A separately pinned phase policy
//! restricts the generation-2 key to one identity and one schedule semantic
//! digest. No private bytes are returned, persisted or logged by this module.
//!
//! This is source composition, NOT installation/credential/phase authority.
//! The fixed launcher must admit the policy from the signed O3 manifest before
//! constructing the service. There is no filesystem discovery or key rotation.

use std::{
    io::{Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use strategy_runtime_core::{
    stage8b_p1e_canonical_json, stage8b_p1e_schedule_unsigned_signature_sha256,
    verify_stage8b_p1e_schedule_envelope_v3, Stage8bP1eScheduleEnvelopeV3,
    Stage8bP1eScheduleVerificationContextV1, STAGE8B_P1E_SCHEDULE_PUBLIC_KEY_ED25519_HEX,
};

use crate::{Stage8bP1eSchedulePublisherError as Error, Stage8bP1eScheduleSigner};
mod service;
pub use service::run_fixed_signer;

pub const O3_SCHEDULE_SIGNER_SOCKET: &str = "/run/moex-finam-p1-o3/schedule-signer.sock";
const MAX_REQUEST_BYTES: usize = 128 * 1024;
const TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct O3ScheduleSigningPolicy {
    pub schema_version: u16,
    pub phase_manifest_sha256: String,
    pub operational_identity_sha256: String,
    pub runtime_config_fingerprint_sha256: String,
    pub instrument_map_fingerprint_sha256: String,
    pub registry_identity_sha256: String,
    pub registry_version: String,
    pub schedule_semantic_sha256: String,
    pub not_before: DateTime<Utc>,
    pub deadline: DateTime<Utc>,
    pub publisher_uid: u32,
}

impl O3ScheduleSigningPolicy {
    fn validate(&self) -> Result<(), Error> {
        let hash = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if self.schema_version != 1
            || ![
                &self.phase_manifest_sha256,
                &self.operational_identity_sha256,
                &self.runtime_config_fingerprint_sha256,
                &self.instrument_map_fingerprint_sha256,
                &self.registry_identity_sha256,
                &self.schedule_semantic_sha256,
            ]
            .into_iter()
            .all(|s| hash(s))
            || self.registry_version.is_empty()
            || self.registry_version.len() > 128
            || self.publisher_uid == 0
            || self.deadline <= self.not_before
            || self.deadline - self.not_before > chrono::Duration::seconds(1800)
        {
            return Err(Error::SigningAuthorityMismatch);
        }
        Ok(())
    }

    fn context(&self, now: DateTime<Utc>) -> Stage8bP1eScheduleVerificationContextV1 {
        Stage8bP1eScheduleVerificationContextV1 {
            expected_instrument_map_fingerprint_sha256: self
                .instrument_map_fingerprint_sha256
                .clone(),
            expected_operational_identity_sha256: self.operational_identity_sha256.clone(),
            expected_registry_identity_sha256: self.registry_identity_sha256.clone(),
            expected_registry_version: self.registry_version.clone(),
            expected_runtime_config_fingerprint_sha256: self
                .runtime_config_fingerprint_sha256
                .clone(),
            high_water: None,
            trusted_now: now,
        }
    }
}

/// Only the dedicated signer service owns this value, never the paper child.
/// Production construction rejects every key except the accepted generation 2.
pub struct O3ScheduleSigningService {
    policy: O3ScheduleSigningPolicy,
    key: SigningKey,
    monotonic_deadline: Instant,
}

impl O3ScheduleSigningService {
    pub fn new(policy: O3ScheduleSigningPolicy, key: SigningKey) -> Result<Self, Error> {
        policy.validate()?;
        if hex(key.verifying_key().as_bytes()) != STAGE8B_P1E_SCHEDULE_PUBLIC_KEY_ED25519_HEX {
            return Err(Error::SigningAuthorityMismatch);
        }
        let remaining = (policy.deadline - Utc::now())
            .to_std()
            .map_err(|_| Error::SigningFailed)?;
        let monotonic_deadline = Instant::now() + remaining.min(Duration::from_secs(1800));
        Ok(Self {
            policy,
            key,
            monotonic_deadline,
        })
    }

    /// One bounded request. The caller owns listener/phase termination and
    /// must not continue accepting after guardian stop. No clock override is
    /// exposed here, and both peers are checked on the connected socket.
    pub fn serve_connection(&self, stream: &mut UnixStream) -> Result<(), Error> {
        if peer_uid(stream)? != self.policy.publisher_uid {
            return Err(Error::SigningAuthorityMismatch);
        }
        deadlines(stream)?;
        let request = read_request(stream)?;
        let signature = self.sign_at(&request, Utc::now(), |bytes, context| {
            verify_stage8b_p1e_schedule_envelope_v3(bytes, context)
                .map(|_| ())
                .map_err(|_| Error::SigningFailed)
        })?;
        if Utc::now() >= self.policy.deadline || Instant::now() >= self.monotonic_deadline {
            return Err(Error::SigningFailed);
        }
        stream
            .write_all(&signature)
            .map_err(|_| Error::SigningFailed)
    }

    fn sign_at(
        &self,
        request: &[u8],
        now: DateTime<Utc>,
        verify: impl FnOnce(&[u8], &Stage8bP1eScheduleVerificationContextV1) -> Result<(), Error>,
    ) -> Result<[u8; 64], Error> {
        self.policy.validate()?;
        if now < self.policy.not_before
            || now >= self.policy.deadline
            || request.len() > MAX_REQUEST_BYTES
        {
            return Err(Error::SigningFailed);
        }
        let mut envelope: Stage8bP1eScheduleEnvelopeV3 =
            serde_json::from_slice(request).map_err(|_| Error::SigningFailed)?;
        if !envelope.signature_ed25519_hex.is_empty()
            || stage8b_p1e_canonical_json(&envelope).map_err(|_| Error::SigningFailed)? != request
            || envelope.operational_identity_sha256 != self.policy.operational_identity_sha256
            || envelope.runtime_config_fingerprint_sha256
                != self.policy.runtime_config_fingerprint_sha256
            || envelope.instrument_map_fingerprint_sha256
                != self.policy.instrument_map_fingerprint_sha256
            || envelope.schedule_semantic_sha256 != self.policy.schedule_semantic_sha256
            || envelope.source_generation != "1"
            || envelope.key_generation != 2
        {
            return Err(Error::SigningAuthorityMismatch);
        }
        let published = DateTime::parse_from_rfc3339(&envelope.published_at_utc)
            .map_err(|_| Error::SigningFailed)?
            .with_timezone(&Utc);
        if published < self.policy.not_before
            || published > now
            || now - published > chrono::Duration::seconds(2)
        {
            return Err(Error::SigningFailed);
        }
        let digest = stage8b_p1e_schedule_unsigned_signature_sha256(&envelope)
            .map_err(|_| Error::SigningFailed)?;
        let signature = self.key.sign(&decode_digest(&digest)?).to_bytes();
        envelope.signature_ed25519_hex = hex(&signature);
        // No signature is released until the ordinary production verifier
        // accepts every envelope/payload/report/freshness constraint.
        verify(
            &stage8b_p1e_canonical_json(&envelope).map_err(|_| Error::SigningFailed)?,
            &self.policy.context(now),
        )?;
        Ok(signature)
    }
}

pub struct O3UnixScheduleSigner;

impl Stage8bP1eScheduleSigner for O3UnixScheduleSigner {
    fn public_key_ed25519_hex(&self) -> &str {
        STAGE8B_P1E_SCHEDULE_PUBLIC_KEY_ED25519_HEX
    }

    fn sign_raw_prehash(&self, _: &[u8; 32]) -> Result<[u8; 64], Error> {
        Err(Error::SigningFailed) // Never an arbitrary-message signing oracle.
    }

    fn sign_schedule_envelope(
        &self,
        envelope: &Stage8bP1eScheduleEnvelopeV3,
    ) -> Result<[u8; 64], Error> {
        let bytes = stage8b_p1e_canonical_json(envelope).map_err(|_| Error::SigningFailed)?;
        let mut stream =
            UnixStream::connect(O3_SCHEDULE_SIGNER_SOCKET).map_err(|_| Error::SigningFailed)?;
        if peer_uid(&stream)? != 0 {
            return Err(Error::SigningAuthorityMismatch);
        }
        exchange(&mut stream, &bytes)
    }
}

fn deadlines(stream: &UnixStream) -> Result<(), Error> {
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|_| Error::SigningFailed)?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|_| Error::SigningFailed)
}

fn exchange(stream: &mut UnixStream, bytes: &[u8]) -> Result<[u8; 64], Error> {
    if bytes.is_empty() || bytes.len() > MAX_REQUEST_BYTES {
        return Err(Error::SigningFailed);
    }
    deadlines(stream)?;
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .map_err(|_| Error::SigningFailed)?;
    stream.write_all(bytes).map_err(|_| Error::SigningFailed)?;
    let mut signature = [0; 64];
    stream
        .read_exact(&mut signature)
        .map_err(|_| Error::SigningFailed)?;
    Ok(signature)
}

fn read_request(stream: &mut UnixStream) -> Result<Vec<u8>, Error> {
    let deadline = Instant::now() + TIMEOUT;
    let mut header = [0; 4];
    read_exact_before(stream, &mut header, deadline)?;
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > MAX_REQUEST_BYTES {
        return Err(Error::SigningFailed);
    }
    let mut bytes = vec![0; length];
    read_exact_before(stream, &mut bytes, deadline)?;
    Ok(bytes)
}

fn read_exact_before(
    stream: &mut UnixStream,
    mut bytes: &mut [u8],
    deadline: Instant,
) -> Result<(), Error> {
    while !bytes.is_empty() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or(Error::SigningFailed)?;
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|_| Error::SigningFailed)?;
        match stream.read(bytes) {
            Ok(0) => return Err(Error::SigningFailed),
            Ok(n) => bytes = &mut bytes[n..],
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(Error::SigningFailed),
        }
    }
    Ok(())
}

fn peer_uid(stream: &UnixStream) -> Result<u32, Error> {
    #[cfg(target_os = "linux")]
    {
        let mut credential: libc::ucred = unsafe { std::mem::zeroed() };
        let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let result = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut credential as *mut libc::ucred).cast(),
                &mut size,
            )
        };
        if result != 0 || size as usize != std::mem::size_of::<libc::ucred>() {
            return Err(Error::SigningFailed);
        }
        Ok(credential.uid)
    }
    #[cfg(target_os = "macos")]
    {
        let mut uid = 0;
        let mut gid = 0;
        if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
            return Err(Error::SigningFailed);
        }
        Ok(uid)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = stream;
        Err(Error::SigningFailed)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn decode_digest(hash: &str) -> Result<[u8; 32], Error> {
    let mut output = [0; 32];
    if hash.len() != 64 || !hash.is_ascii() {
        return Err(Error::SigningFailed);
    }
    for (i, byte) in output.iter_mut().enumerate() {
        *byte =
            u8::from_str_radix(&hash[2 * i..2 * i + 2], 16).map_err(|_| Error::SigningFailed)?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage8b_p1e_schedule_publisher::{
        authorize_stage8b_p1e_first_publication,
        test_prepare_stage8b_p1e_schedule_publication_with_key,
        tests::{fixture_input, FixtureSigner},
        Stage8bP1eSchedulePublisherLineage,
    };

    fn fixture() -> (O3ScheduleSigningService, Vec<u8>, DateTime<Utc>) {
        let now = DateTime::parse_from_rfc3339("2026-09-14T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let signer = FixtureSigner::new(91);
        let state = test_prepare_stage8b_p1e_schedule_publication_with_key(
            fixture_input(now, "2026-09-14T20:50:00.000000Z"),
            Stage8bP1eSchedulePublisherLineage::First(
                authorize_stage8b_p1e_first_publication("AUTHORIZE-STAGE8B-P1E-FIRST-PUBLICATION")
                    .unwrap(),
            ),
            &signer,
            signer.public_key_ed25519_hex(),
        )
        .unwrap();
        let mut envelope: Stage8bP1eScheduleEnvelopeV3 =
            serde_json::from_slice(&state.exact_envelope_bytes().unwrap()).unwrap();
        envelope.signature_ed25519_hex.clear();
        let policy = O3ScheduleSigningPolicy {
            schema_version: 1,
            phase_manifest_sha256: "a".repeat(64),
            operational_identity_sha256: envelope.operational_identity_sha256.clone(),
            runtime_config_fingerprint_sha256: envelope.runtime_config_fingerprint_sha256.clone(),
            instrument_map_fingerprint_sha256: envelope.instrument_map_fingerprint_sha256.clone(),
            registry_identity_sha256: envelope.payload.registry.registry_identity_sha256.clone(),
            registry_version: envelope.payload.registry.registry_version.clone(),
            schedule_semantic_sha256: envelope.schedule_semantic_sha256.clone(),
            not_before: now - chrono::Duration::seconds(1),
            deadline: now + chrono::Duration::seconds(1799),
            publisher_uid: 987,
        };
        let key = SigningKey::from_bytes(&[91; 32]);
        assert!(O3ScheduleSigningService::new(policy.clone(), key.clone()).is_err());
        // Test-only struct construction; public production construction above
        // demonstrably rejects this fixture key. Not O3 executable evidence.
        (
            O3ScheduleSigningService {
                policy,
                key,
                monotonic_deadline: Instant::now() + Duration::from_secs(60),
            },
            stage8b_p1e_canonical_json(&envelope).unwrap(),
            now,
        )
    }

    fn fixture_verify(
        bytes: &[u8],
        context: &Stage8bP1eScheduleVerificationContextV1,
    ) -> Result<(), Error> {
        let signer = FixtureSigner::new(91);
        strategy_runtime_core::stage8b_p1e_test_verify_schedule_envelope_with_key(
            bytes,
            context,
            signer.public_key_ed25519_hex(),
            DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            DateTime::parse_from_rfc3339("2027-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        )
        .map(|_| ())
        .map_err(|_| Error::SigningFailed)
    }

    #[test]
    fn o3_signer_policy_rejects_raw_digest_foreign_identity_stale_and_bad_report() {
        let (service, request, now) = fixture();
        assert!(O3UnixScheduleSigner.sign_raw_prehash(&[0; 32]).is_err());
        assert!(service.sign_at(&request, now, fixture_verify).is_ok());
        assert!(service
            .sign_at(&request, service.policy.deadline, fixture_verify)
            .is_err());
        assert!(service
            .sign_at(
                &request,
                service.policy.not_before - chrono::Duration::seconds(1),
                fixture_verify
            )
            .is_err());
        assert!(service
            .sign_at(&request, now + chrono::Duration::seconds(3), fixture_verify)
            .is_err());
        assert!(service.sign_at(b"{}", now, fixture_verify).is_err());
        for field in [
            "operational_identity_sha256",
            "runtime_config_fingerprint_sha256",
            "instrument_map_fingerprint_sha256",
            "schedule_semantic_sha256",
        ] {
            let mut value: serde_json::Value = serde_json::from_slice(&request).unwrap();
            value[field] = serde_json::json!("f".repeat(64));
            assert!(
                service
                    .sign_at(
                        &stage8b_p1e_canonical_json(&value).unwrap(),
                        now,
                        fixture_verify
                    )
                    .is_err(),
                "{field}"
            );
        }
        for (field, wrong) in [
            ("source_generation", serde_json::json!("2")),
            ("key_generation", serde_json::json!(1)),
            ("schema_version", serde_json::json!(2)),
            ("signature_ed25519_hex", serde_json::json!("a".repeat(128))),
        ] {
            let mut value: serde_json::Value = serde_json::from_slice(&request).unwrap();
            value[field] = wrong;
            assert!(
                service
                    .sign_at(
                        &stage8b_p1e_canonical_json(&value).unwrap(),
                        now,
                        fixture_verify
                    )
                    .is_err(),
                "{field}"
            );
        }
        let mut value: serde_json::Value = serde_json::from_slice(&request).unwrap();
        value["payload"]["stage4_evidence"]["report_sha256"] = serde_json::json!("f".repeat(64));
        assert!(service
            .sign_at(
                &stage8b_p1e_canonical_json(&value).unwrap(),
                now,
                fixture_verify
            )
            .is_err());
        assert!(service
            .sign_at(&request, now, |_, _| Err(Error::SigningFailed))
            .is_err());
    }

    #[test]
    fn o3_signer_unix_framing_is_bounded_and_does_not_accept_digest_requests() {
        let (service, request, now) = fixture();
        let expected = service.sign_at(&request, now, fixture_verify).unwrap();
        let (mut client, mut server) = UnixStream::pair().unwrap();
        assert_eq!(peer_uid(&client).unwrap(), unsafe { libc::geteuid() });
        let expected_request = request.clone();
        let worker = std::thread::spawn(move || {
            deadlines(&server).unwrap();
            let bytes = read_request(&mut server).unwrap();
            assert_eq!(bytes, expected_request);
            let signature = service.sign_at(&bytes, now, fixture_verify).unwrap();
            server.write_all(&signature).unwrap();
        });
        assert_eq!(exchange(&mut client, &request).unwrap(), expected);
        worker.join().unwrap();
        for length in [0, MAX_REQUEST_BYTES as u32 + 1, u32::MAX] {
            let (mut client, mut server) = UnixStream::pair().unwrap();
            client.write_all(&length.to_be_bytes()).unwrap();
            assert!(read_request(&mut server).is_err());
        }
        let (mut client, mut server) = UnixStream::pair().unwrap();
        client.write_all(&100u32.to_be_bytes()).unwrap();
        client.write_all(b"short").unwrap();
        drop(client);
        assert!(read_request(&mut server).is_err());
    }
}
