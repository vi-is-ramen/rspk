//! SBOM (Software Bill of Materials) generation for `pk`.
//!
//! Generates machine-readable inventories of all installed packages
//! across every discovered package manager. Supports two industry
//! standards:
//!
//! - [CycloneDX 1.6](https://cyclonedx.org/specification/overview/) —
//!   lightweight, JSON-native, widely adopted in CI/CD and security
//!   tooling (OWASP Dependency-Track, Trivy, Grype).
//! - [SPDX 2.3](https://spdx.github.io/spdx-spec/v2.3/) — ISO/IEC
//!   5962, preferred in legal/compliance contexts.
//!
//! Every component carries a [PURL](https://github.com/package-url/purl-spec)
//! (Package URL) for unambiguous identification across ecosystems.
//!
//! # Example
//!
//! ```no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use rspk_sbom::{SbomFormat, generate};
//! use rspk_managers::ManagerRegistry;
//! use rspk_core::ExecutionContext;
//!
//! let registry = ManagerRegistry::discover().await?;
//! let ctx = ExecutionContext::new();
//! let sbom = generate(&registry, &ctx, None).await?;
//! println!("{}", sbom.to_json(SbomFormat::CycloneDx)?);
//! # Ok(())
//! # }
//! ```
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod cyclonedx;
pub mod generate;
pub mod purl;
pub mod spdx;

pub use cyclonedx::CycloneDxBom;
pub use generate::{SbomFormat, SbomOptions, generate};
pub use purl::Purl;
pub use spdx::SpdxDocument;
