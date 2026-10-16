//! Typed failure boundaries for the pinned host-side CLVM test VM.
//!
//! A VM failure is not automatically a policy rejection. Resource exhaustion
//! and infrastructure/codec errors cannot confirm an abstract rejection claim.
#![forbid(unsafe_code)]

use clvmr::error::EvalErr;
use serde::{Deserialize, Serialize, Serializer, ser::SerializeStruct};

pub const FAILURE_PROTOCOL: &str = "clvmr-0.21.0/v1";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Program,
    Resource,
    Infrastructure,
}

macro_rules! failure_codes {
    ($($(#[$attribute:meta])* $code:ident => $kind:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
        #[serde(rename_all = "snake_case")]
        pub enum FailureCode { $($(#[$attribute])* $code),+ }

        impl FailureCode {
            pub const ALL: &'static [Self] = &[$(Self::$code),+];

            pub const fn kind(self) -> FailureKind {
                match self { $(Self::$code => FailureKind::$kind),+ }
            }
        }
    };
}

failure_codes! {
    SerializationError => Infrastructure,
    NonCanonicalSerialization => Infrastructure,
    SerializationBackreferenceError => Infrastructure,
    OutOfMemory => Resource,
    PathIntoAtom => Program,
    TooManyPairs => Resource,
    TooManyAtoms => Resource,
    CostExceeded => Resource,
    Timeout => Resource,
    UnknownSoftforkExtension => Program,
    SoftforkCostMismatch => Program,
    InternalError => Infrastructure,
    Raise => Program,
    InvalidNilTerminator => Program,
    DivisionByZero => Program,
    ValueStackLimitReached => Resource,
    EnvironmentStackLimitReached => Resource,
    ShiftTooLarge => Program,
    Reserved => Program,
    Invalid => Program,
    Unimplemented => Program,
    InvalidOpArg => Program,
    InvalidAllocArg => Program,
    #[serde(rename = "bls_pairing_identity_failed")]
    BLSPairingIdentityFailed => Program,
    #[serde(rename = "bls_verify_failed")]
    BLSVerifyFailed => Program,
    Secp256Failed => Program,
    SoftforkStackDepthExceeded => Resource,
}

/// A failure code determines its kind. The wire decoder rejects contradictory
/// or unknown classifications, rather than trusting a caller-supplied category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmFailure {
    code: FailureCode,
}

impl VmFailure {
    pub const fn from_code(code: FailureCode) -> Self {
        Self { code }
    }

    pub const fn code(self) -> FailureCode {
        self.code
    }

    pub const fn kind(self) -> FailureKind {
        self.code.kind()
    }

    /// This denotes a VM-semantic rejection, not proof of an intended policy.
    pub const fn is_program_rejection(self) -> bool {
        matches!(self.kind(), FailureKind::Program)
    }
}

impl Serialize for VmFailure {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut record = serializer.serialize_struct("VmFailure", 2)?;
        record.serialize_field("kind", &self.kind())?;
        record.serialize_field("code", &self.code)?;
        record.end()
    }
}

impl<'de> Deserialize<'de> for VmFailure {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            kind: FailureKind,
            code: FailureCode,
        }
        let wire = Wire::deserialize(deserializer)?;
        if wire.kind != wire.code.kind() {
            return Err(serde::de::Error::custom(
                "VM failure kind contradicts its code",
            ));
        }
        Ok(Self::from_code(wire.code))
    }
}

/// Exhaustively classify the pinned VM's typed errors. An upstream enum change
/// requires an explicit review; there is no catch-all "program rejected" arm.
///
/// InvalidAllocArg is operand validation in this VM (substring bounds or curve
/// point encoding). ShiftTooLarge is its explicit shift-operand restriction.
/// Codec failures and InternalError are conservatively infrastructure failures.
pub fn classify_failure(error: &EvalErr) -> VmFailure {
    let code = match error {
        EvalErr::SerializationError => FailureCode::SerializationError,
        EvalErr::NonCanonicalSerialization => FailureCode::NonCanonicalSerialization,
        EvalErr::SerializationBackreferenceError => FailureCode::SerializationBackreferenceError,
        EvalErr::OutOfMemory => FailureCode::OutOfMemory,
        EvalErr::PathIntoAtom => FailureCode::PathIntoAtom,
        EvalErr::TooManyPairs => FailureCode::TooManyPairs,
        EvalErr::TooManyAtoms => FailureCode::TooManyAtoms,
        EvalErr::CostExceeded => FailureCode::CostExceeded,
        EvalErr::Timeout => FailureCode::Timeout,
        EvalErr::UnknownSoftforkExtension => FailureCode::UnknownSoftforkExtension,
        EvalErr::SoftforkCostMismatch => FailureCode::SoftforkCostMismatch,
        EvalErr::InternalError(..) => FailureCode::InternalError,
        EvalErr::Raise(..) => FailureCode::Raise,
        EvalErr::InvalidNilTerminator(..) => FailureCode::InvalidNilTerminator,
        EvalErr::DivisionByZero(..) => FailureCode::DivisionByZero,
        EvalErr::ValueStackLimitReached(..) => FailureCode::ValueStackLimitReached,
        EvalErr::EnvironmentStackLimitReached(..) => FailureCode::EnvironmentStackLimitReached,
        EvalErr::ShiftTooLarge(..) => FailureCode::ShiftTooLarge,
        EvalErr::Reserved(..) => FailureCode::Reserved,
        EvalErr::Invalid(..) => FailureCode::Invalid,
        EvalErr::Unimplemented(..) => FailureCode::Unimplemented,
        EvalErr::InvalidOpArg(..) => FailureCode::InvalidOpArg,
        EvalErr::InvalidAllocArg(..) => FailureCode::InvalidAllocArg,
        EvalErr::BLSPairingIdentityFailed(..) => FailureCode::BLSPairingIdentityFailed,
        EvalErr::BLSVerifyFailed(..) => FailureCode::BLSVerifyFailed,
        EvalErr::Secp256Failed(..) => FailureCode::Secp256Failed,
        EvalErr::SoftforkStackDepthExceeded => FailureCode::SoftforkStackDepthExceeded,
    };
    VmFailure::from_code(code)
}

#[cfg(test)]
mod protocol_contract {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn cross_language_failure_protocol_matches_the_typed_wire_contract() {
        let declared: Value =
            serde_json::from_str(include_str!("../failure-protocol.json")).unwrap();
        let failures = FailureCode::ALL
            .iter()
            .map(|code| VmFailure::from_code(*code))
            .collect::<Vec<_>>();
        assert_eq!(
            declared,
            json!({
                "schema_version": 1,
                "protocol": FAILURE_PROTOCOL,
                "failures": failures,
            })
        );
        for failure in failures {
            let wire = serde_json::to_string(&failure).unwrap();
            assert_eq!(serde_json::from_str::<VmFailure>(&wire).unwrap(), failure);
        }
    }

    #[test]
    fn deserialization_rejects_forged_or_unknown_failure_classifications() {
        for wire in [
            r#"{"kind":"program","code":"cost_exceeded"}"#,
            r#"{"kind":"resource","code":"raise"}"#,
            r#"{"kind":"program","code":"future_unknown_error"}"#,
            r#"{"kind":"program","code":"raise","extra":true}"#,
            r#"{"kind":"program","kind":"resource","code":"raise"}"#,
            r#"{"code":"raise"}"#,
        ] {
            assert!(serde_json::from_str::<VmFailure>(wire).is_err(), "{wire}");
        }
    }
}
