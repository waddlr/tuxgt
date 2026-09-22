#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("fluent: {0}")]
    Fluent(String),
    #[error("unknown plugin: {0}")]
    UnknownPlugin(String),
    #[error("invalid plugin id: {0}")]
    InvalidPluginId(String),
    #[error("invalid plugin descriptor: {0}")]
    InvalidPluginDesc(String),
    #[error("toml: {0}")]
    Toml(String),
    #[error("invalid game id: {0}")]
    InvalidGameId(String),
    #[error("invalid mod type: {0}")]
    InvalidModType(String),
    #[error("invalid slot: {0}")]
    InvalidSlot(String),
    #[error("invalid instance: {0}")]
    InvalidInstance(String),
    #[error("unknown instance: {0}")]
    UnknownInstance(String),
    #[error("download: {0}")]
    Download(String),
    #[error("cache: {0}")]
    Cache(String),
    #[error("manifest: {0}")]
    Manifest(String),
    #[error("staging modified (re-run with --force to overwrite): {0}")]
    StagedModified(String),
    #[error("no manifest for {0}")]
    NoManifest(String),
    #[error("sha256 mismatch: want {want}, got {got}")]
    HashMismatch { want: String, got: String },
    #[error("missing tool: {0}")]
    MissingTool(String),
    #[error("unpack: {0}")]
    Unpack(String),
    #[error("archive password required")]
    ArchivePasswordRequired,
    #[error("apply is not implemented for {0}")]
    ApplyUnsupported(String),
    #[error("apply: {0}")]
    Apply(String),
    #[error("plugin disabled: {0}")]
    PluginDisabled(String),
    #[error("instance disabled: {0}")]
    InstanceDisabled(String),
    #[error("not a file: {0}")]
    NotAFile(String),
    #[error("unknown game: {0}")]
    UnknownGame(String),
    #[error("no executable for {0}")]
    MissingExe(String),
    #[error("tuxgt-launcher not found")]
    MissingLauncher,
    #[error("steam client not found")]
    MissingSteam,
    #[error("heroic client not found")]
    MissingHeroic,
    #[error("no proton/wine runner for {0}")]
    MissingRunner(String),
    #[error("metadata fetch: {0}")]
    Fetch(String),
    #[error("keyring: {0}")]
    SecretManager(String),
    #[error("unknown metadata source: {0}")]
    UnknownMetadataSource(String),
    #[error("game env is not a JSON object: {0}")]
    BadEnv(String),
    #[error("unknown knob: {0}")]
    UnknownKnob(String),
    #[error("invalid knob value: {0}")]
    InvalidKnobValue(String),
    #[error("invalid override: {0}")]
    InvalidOverride(String),
    #[error("correlator collision: {0}")]
    CorrelatorCollision(String),
    #[error("knob not applicable: {0}")]
    KnobNotApplicable(String),
    #[error("knob not set: {0}")]
    KnobNotSet(String),
    #[error("knob unmanaged: {0}")]
    KnobUnmanaged(String),
    #[error("unknown wrapper: {0}")]
    UnknownWrapper(String),
    #[error("wrapper not set: {0}")]
    WrapperNotSet(String),
    #[error("custom env not set: {0}")]
    CustomEnvNotSet(String),
    #[error("invalid env key: {0}")]
    InvalidEnvKey(String),
    #[error("{0}")]
    NeedConfirm(String),
    #[error("install: {0}")]
    Install(String),
    #[error("missing requires type {0}")]
    MissingRequires(String),
}

pub type Result<T> = std::result::Result<T, Error>;
