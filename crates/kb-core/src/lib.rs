//! Domain types and deterministic rules shared by every Knowledge-Brain entry point.

mod admission;
mod backup;
mod config;
mod error;
mod extraction;
mod okf;
mod operation;
mod path;
mod platform;
mod search;
mod source;
mod version;
pub use extraction::{
    ExtractedBlock, ExtractedDocument, ExtractedLink, ExtractionStatus, Extractor, MediaType,
    SourceLocation,
};
pub use search::{
    Catalog, CatalogEntry, SearchBackend, SearchExplanation, SearchField, SearchFieldContribution,
    SearchGroup, SearchHit, SearchRequest, SearchResponse, SearchScope,
};
pub use source::{SourceId, SourceVersion};

pub use admission::{AdmissionDocument, AdmissionEntry};
pub use backup::{
    BackupCreateReport, BackupFile, BackupManifest, BackupRestoreReport, BackupVerifyReport,
};
pub use config::{
    ConfigSource, EffectiveConfig, EffectiveFiles, EffectiveLimits, EffectiveOperations,
    EffectiveSearch, PartialConfig, PartialFiles, PartialLimits, PartialOperations, PartialSearch,
    SearchMode, Sourced,
};
pub use error::{ErrorCode, KbError};
pub use okf::{
    MarkdownLink, OkfDocumentKind, OkfFinding, OkfSeverity, OkfSourceResource, ParsedOkfDocument,
    parse_okf, validate_okf,
};
pub use operation::{
    AdoptionPlan, KnowledgeChangeRequest, KnowledgePlan, KnowledgePlanRequest, KnowledgePlanResult,
    KnowledgeWrite, ObservedEntry, ObservedKind, OperationEvent, OperationEventKind,
    OperationEventLog, OperationEventReport, OperationId, OperationKind, PlannedFile,
};
pub use path::{
    PortabilityCollision, PortableRelativePath, detect_portability_collisions, find_vault_root,
    portability_key, validate_admission_directory, validate_generated_path,
};
pub use platform::ensure_not_link_or_reparse_point;
pub use version::{
    CURRENT_SCHEMA_VERSION, SchemaCompatibility, SchemaVersion, SchemaVersionParseError,
};
