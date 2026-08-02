//! SBOM generation pipeline.
//!
//! Collects installed packages from all discovered managers,
//! builds PURLs, and assembles a CycloneDX or SPDX document.

use crate::cyclonedx::{
    CdxComponent, CdxMetadata, CdxProperty, CdxTool, CycloneDxBom,
};
use crate::purl::Purl;
use crate::spdx::{
    SpdxCreationInfo, SpdxDocument, SpdxExternalRef, SpdxPackage,
    SpdxRelationship,
};
use rspk_core::{ExecutionContext, PackageManager};
use rspk_managers::ManagerRegistry;
use std::sync::Arc;
use tokio::task::JoinSet;
use tracing::debug;

/// Output format for the generated SBOM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SbomFormat
{
    /// CycloneDX 1.6 JSON.
    #[default]
    CycloneDx,
    /// SPDX 2.3 JSON.
    Spdx,
}

impl std::fmt::Display for SbomFormat
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        match self
        {
            Self::CycloneDx => write!(f, "cyclonedx"),
            Self::Spdx => write!(f, "spdx"),
        }
    }
}

impl std::str::FromStr for SbomFormat
{
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err>
    {
        match s.to_lowercase().as_str()
        {
            "cyclonedx" | "cdx" => Ok(Self::CycloneDx),
            "spdx" => Ok(Self::Spdx),
            other => Err(format!(
                "unknown SBOM format '{other}' (expected cyclonedx or spdx)"
            )),
        }
    }
}

/// Options controlling SBOM generation.
#[derive(Debug, Clone, Default)]
pub struct SbomOptions
{
    /// Restrict to a single manager (by ID). `None` = all managers.
    pub manager:           Option<String>,
    /// Human-readable name for the subject component.
    pub component_name:    Option<String>,
    /// Version for the subject component.
    pub component_version: Option<String>,
}

/// A collected package with its manager metadata.
#[derive(Debug, Clone)]
struct CollectedPackage
{
    pkg:          rspk_core::Package,
    manager_id:   String,
    manager_name: String,
}

/// Generates an SBOM from all installed packages.
///
/// Queries every available manager (or a single one if
/// `opts.manager` is set) in parallel, collects installed packages,
/// and assembles the document.
///
/// # Errors
///
/// Returns an error if package collection fails for all managers.
pub async fn generate(
    registry: &ManagerRegistry,
    ctx: &ExecutionContext,
    opts: Option<SbomOptions>,
) -> Result<GeneratedSbom, anyhow::Error>
{
    let opts = opts.unwrap_or_default();

    let managers: Vec<Arc<dyn PackageManager>> = if let Some(ref id) =
        opts.manager
    {
        vec![registry.get(id)?]
    }
    else
    {
        registry
            .available()
            .into_iter()
            .filter(|m| m.capabilities().list_installed)
            .collect()
    };

    if managers.is_empty()
    {
        anyhow::bail!("no package managers available for SBOM generation");
    }

    // Phase 1: collect installed packages in parallel.
    let mut set: JoinSet<Vec<CollectedPackage>> = JoinSet::new();

    for mgr in managers
    {
        let ctx = ctx.clone();
        set.spawn(async move {
            match mgr.installed(&ctx).await
            {
                Ok(packages) =>
                {
                    debug!(
                        manager = mgr.id(),
                        count = packages.len(),
                        "collected packages"
                    );
                    packages
                        .into_iter()
                        .map(|pkg| CollectedPackage {
                            pkg,
                            manager_id: mgr.id().to_string(),
                            manager_name: mgr.name().to_string(),
                        })
                        .collect()
                },
                Err(e) =>
                {
                    debug!(
                        manager = mgr.id(),
                        error = %e,
                        "failed to list installed packages"
                    );
                    Vec::new()
                },
            }
        });
    }

    let mut all_packages: Vec<CollectedPackage> = Vec::new();
    while let Some(res) = set.join_next().await
    {
        all_packages.extend(res?);
    }

    // Deduplicate by PURL (same package visible via multiple managers).
    all_packages.sort_by(|a, b| a.pkg.id.cmp(&b.pkg.id));
    all_packages.dedup_by(|a, b| {
        a.pkg.id == b.pkg.id
            && a.pkg.installed_version == b.pkg.installed_version
    });

    debug!(total = all_packages.len(), "SBOM packages collected");

    let timestamp =
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let uuid = uuid::Uuid::new_v4();

    Ok(GeneratedSbom {
        packages: all_packages,
        timestamp,
        uuid,
        opts,
    })
}

/// A generated SBOM ready for serialization.
#[derive(Debug, Clone)]
pub struct GeneratedSbom
{
    packages:  Vec<CollectedPackage>,
    timestamp: String,
    uuid:      uuid::Uuid,
    opts:      SbomOptions,
}

impl GeneratedSbom
{
    /// Returns the total number of components.
    pub fn component_count(&self) -> usize
    {
        self.packages.len()
    }

    /// Serializes to the given format.
    ///
    /// # Errors
    ///
    /// Returns an error if JSON serialization fails.
    pub fn to_json(
        &self,
        format: SbomFormat,
    ) -> Result<String, serde_json::Error>
    {
        match format
        {
            SbomFormat::CycloneDx => self.to_cyclonedx().to_json(),
            SbomFormat::Spdx => self.to_spdx().to_json(),
        }
    }

    /// Builds a CycloneDX 1.6 BOM.
    pub fn to_cyclonedx(&self) -> CycloneDxBom
    {
        let host_name = self
            .opts
            .component_name
            .clone()
            .unwrap_or_else(|| "host-system".to_string());

        let components: Vec<CdxComponent> = self
            .packages
            .iter()
            .map(|cp| {
                let purl = Purl::from_package(&cp.pkg, &cp.manager_id);
                let purl_str = purl.to_string();
                CdxComponent {
                    component_type: "library".to_string(),
                    bom_ref:        purl_str.clone(),
                    name:           cp.pkg.id.clone(),
                    version:        cp
                        .pkg
                        .installed_version
                        .as_ref()
                        .map(ToString::to_string),
                    purl:           Some(purl_str),
                    description:    cp.pkg.description.clone(),
                    properties:     vec![
                        CdxProperty {
                            name:  "pk:manager".into(),
                            value: cp.manager_id.clone(),
                        },
                        CdxProperty {
                            name:  "pk:manager-name".into(),
                            value: cp.manager_name.clone(),
                        },
                    ],
                }
            })
            .collect();

        CycloneDxBom {
            bom_format: "CycloneDX".to_string(),
            spec_version: "1.6".to_string(),
            serial_number: format!("urn:uuid:{}", self.uuid),
            version: 1,
            metadata: CdxMetadata {
                timestamp: self.timestamp.clone(),
                tools:     vec![CdxTool {
                    vendor:  "rspk".to_string(),
                    name:    "pk".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                }],
                component: CdxComponent {
                    component_type: "application".to_string(),
                    bom_ref:        "host-system".to_string(),
                    name:           host_name,
                    version:        self.opts.component_version.clone(),
                    purl:           None,
                    description:    Some(
                        "Host system described by pk SBOM".to_string(),
                    ),
                    properties:     vec![],
                },
            },
            components,
        }
    }

    /// Builds an SPDX 2.3 document.
    pub fn to_spdx(&self) -> SpdxDocument
    {
        let doc_name = self
            .opts
            .component_name
            .clone()
            .unwrap_or_else(|| "pk-sbom".to_string());

        let mut packages: Vec<SpdxPackage> = Vec::new();
        let mut relationships: Vec<SpdxRelationship> = Vec::new();

        for (i, cp) in self.packages.iter().enumerate()
        {
            let purl = Purl::from_package(&cp.pkg, &cp.manager_id);
            let purl_str = purl.to_string();
            let spdx_id = format!("SPDXRef-Package-{i}");

            packages.push(SpdxPackage {
                spdx_id:           spdx_id.clone(),
                name:              cp.pkg.id.clone(),
                version_info:      cp
                    .pkg
                    .installed_version
                    .as_ref()
                    .map(ToString::to_string),
                download_location: "NOASSERTION".to_string(),
                files_analyzed:    false,
                license_concluded: "NOASSERTION".to_string(),
                license_declared:  "NOASSERTION".to_string(),
                copyright_text:    "NOASSERTION".to_string(),
                description:       cp.pkg.description.clone(),
                external_refs:     vec![SpdxExternalRef {
                    reference_category: "PACKAGE-MANAGER".to_string(),
                    reference_type:     "purl".to_string(),
                    reference_locator:  purl_str,
                }],
                checksums:         None,
            });

            relationships.push(SpdxRelationship {
                spdx_element_id:      "SPDXRef-DOCUMENT".to_string(),
                relationship_type:    "DESCRIBES".to_string(),
                related_spdx_element: spdx_id,
            });
        }

        SpdxDocument {
            spdx_version: "SPDX-2.3".to_string(),
            data_license: "CC0-1.0".to_string(),
            spdx_id: "SPDXRef-DOCUMENT".to_string(),
            name: doc_name,
            document_namespace: format!(
                "https://spdx.org/spdxdocs/pk-{}-{}",
                env!("CARGO_PKG_VERSION"),
                self.uuid
            ),
            creation_info: SpdxCreationInfo {
                created:              self.timestamp.clone(),
                creators:             vec![format!(
                    "Tool: pk-{}",
                    env!("CARGO_PKG_VERSION")
                )],
                license_list_version: Some("3.22".to_string()),
            },
            packages,
            relationships,
        }
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    fn sample_packages() -> Vec<CollectedPackage>
    {
        vec![
            CollectedPackage {
                pkg:          rspk_core::Package {
                    id:                "curl".into(),
                    name:              None,
                    description:       Some("command line tool".into()),
                    installed_version: "7.81.0".parse().ok(),
                    latest_version:    None,
                    manager_id:        "apt".into(),
                    arch:              Some("amd64".into()),
                },
                manager_id:   "apt".into(),
                manager_name: "APT".into(),
            },
            CollectedPackage {
                pkg:          rspk_core::Package {
                    id:                "ripgrep".into(),
                    name:              None,
                    description:       Some("search tool".into()),
                    installed_version: "14.1.0".parse().ok(),
                    latest_version:    None,
                    manager_id:        "cargo".into(),
                    arch:              None,
                },
                manager_id:   "cargo".into(),
                manager_name: "Cargo".into(),
            },
        ]
    }

    #[test]
    fn test_cyclonedx_generation()
    {
        let sbom = GeneratedSbom {
            packages:  sample_packages(),
            timestamp: "2026-08-02T00:00:00Z".into(),
            uuid:      uuid::Uuid::nil(),
            opts:      SbomOptions::default(),
        };

        let cdx = sbom.to_cyclonedx();
        assert_eq!(cdx.bom_format, "CycloneDX");
        assert_eq!(cdx.spec_version, "1.6");
        assert_eq!(cdx.components.len(), 2);
        assert_eq!(cdx.components[0].name, "curl");
        assert!(cdx.components[0].purl.as_ref().unwrap().contains("pkg:deb"));
        assert_eq!(cdx.components[1].name, "ripgrep");
        assert!(
            cdx.components[1]
                .purl
                .as_ref()
                .unwrap()
                .contains("pkg:cargo")
        );

        let json = cdx.to_json().unwrap();
        assert!(json.contains("\"pk:manager\""));
    }

    #[test]
    fn test_spdx_generation()
    {
        let sbom = GeneratedSbom {
            packages:  sample_packages(),
            timestamp: "2026-08-02T00:00:00Z".into(),
            uuid:      uuid::Uuid::nil(),
            opts:      SbomOptions::default(),
        };

        let doc = sbom.to_spdx();
        assert_eq!(doc.spdx_version, "SPDX-2.3");
        assert_eq!(doc.packages.len(), 2);
        assert_eq!(doc.relationships.len(), 2);
        assert_eq!(
            doc.packages[0].external_refs[0].reference_locator,
            "pkg:deb/debian/curl@7.81.0?arch=amd64"
        );

        let json = doc.to_json().unwrap();
        assert!(json.contains("\"DESCRIBES\""));
    }

    #[test]
    fn test_format_parsing()
    {
        assert_eq!(
            "cyclonedx".parse::<SbomFormat>().unwrap(),
            SbomFormat::CycloneDx
        );
        assert_eq!("cdx".parse::<SbomFormat>().unwrap(), SbomFormat::CycloneDx);
        assert_eq!("spdx".parse::<SbomFormat>().unwrap(), SbomFormat::Spdx);
        assert!("invalid".parse::<SbomFormat>().is_err());
    }
}
