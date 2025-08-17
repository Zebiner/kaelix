/// Comprehensive threat model covering various attack vectors
#[derive(Debug, Clone, PartialEq)]
pub enum ThreatModel {
    // Authentication Threats
    WeakCredentials,
    CredentialBruteForce,
    SessionHijacking,
    TokenTampering,

    // Authorization Threats
    PrivilegeEscalation,
    UnauthorizedAccess,
    ResourceExhaustion,

    // Cryptographic Threats
    InsecureRandomness,
    CryptographicDowngrade,
    KeyCompromise,
    SideChannelAttack,

    // Network Threats
    ManInTheMiddle,
    ProtocolDowngrade,
    DNSPoisoning,
    ReplayAttack,

    // Input Validation Threats
    SqlInjection,
    CrossSiteScripting,
    CommandInjection,
    BufferOverflow,

    // Data Protection Threats
    DataLeakage,
    ConfigExposure,
    SensitiveDataLeak,

    // Custom or Advanced Threats
    Custom(String),
}

impl ThreatModel {
    pub fn severity(&self) -> u8 {
        match self {
            ThreatModel::KeyCompromise => 10,
            ThreatModel::PrivilegeEscalation => 9,
            ThreatModel::UnauthorizedAccess => 8,
            ThreatModel::ManInTheMiddle => 8,
            ThreatModel::CredentialBruteForce => 7,
            ThreatModel::DataLeakage => 7,
            ThreatModel::SqlInjection => 6,
            ThreatModel::CrossSiteScripting => 6,
            ThreatModel::SessionHijacking => 6,
            ThreatModel::TokenTampering => 5,
            ThreatModel::ResourceExhaustion => 5,
            ThreatModel::CryptographicDowngrade => 5,
            ThreatModel::InsecureRandomness => 4,
            ThreatModel::SideChannelAttack => 4,
            ThreatModel::BufferOverflow => 3,
            ThreatModel::WeakCredentials => 3,
            ThreatModel::ProtocolDowngrade => 3,
            ThreatModel::DNSPoisoning => 2,
            ThreatModel::ReplayAttack => 2,
            ThreatModel::CommandInjection => 2,
            ThreatModel::ConfigExposure => 1,
            ThreatModel::SensitiveDataLeak => 1,
            ThreatModel::Custom(_) => 0,
        }
    }
}
