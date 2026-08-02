//! SPDX 2.3 document model.
//!
//! Implements the subset of SPDX needed for a system-level SBOM.
//! Uses the JSON serialization format defined in the SPDX 2.3
//! specification (Annex E).

use serde::Serialize;

/// Top-level SPDX document.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpdxDocument
{
    /// Always `"SPDX-2.3"`.
    pub spdx_version:       String,
    /// Data license (always `CC0-1.0` per spec).
    pub data_license:       String,
    /// Document identifier.
    pub spdx_id:            String,
    /// Human-readable document name.
    pub name:               String,
    /// Document namespace URI.
    pub document_namespace: String,
    /// Creation information.
    pub creation_info:      SpdxCreationInfo,
    /// Package entries.
    pub packages:           Vec<SpdxPackage>,
    /// Relationships between elements.
    pub relationships:      Vec<SpdxRelationship>,
}

/// SPDX creation metadata.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpdxCreationInfo
{
    /// ISO 8601 creation timestamp.
    pub created:              String,
    /// Tools/persons that created the document.
    pub creators:             Vec<String>,
    /// License list version used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license_list_version: Option<String>,
}

/// An SPDX package entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpdxPackage
{
    /// SPDX element ID (e.g. `SPDXRef-Package-curl`).
    pub spdx_id:           String,
    /// Package name.
    pub name:              String,
    /// Package version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_info:      Option<String>,
    /// Download location (`NOASSERTION` when unknown).
    pub download_location: String,
    /// Whether the package files were analyzed.
    pub files_analyzed:    bool,
    /// Concluded license (`NOASSERTION` when unknown).
    pub license_concluded: String,
    /// Declared license (`NOASSERTION` when unknown).
    pub license_declared:  String,
    /// Copyright text (`NOASSERTION` when unknown).
    pub copyright_text:    String,
    /// Package description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description:       Option<String>,
    /// External references (PURL).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub external_refs:     Vec<SpdxExternalRef>,
    /// Checksum (SHA-256 of package metadata, if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksums:         Option<Vec<SpdxChecksum>>,
}

/// An external reference (PURL) attached to a package.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpdxExternalRef
{
    /// Reference category.
    pub reference_category: String,
    /// Reference type.
    pub reference_type:     String,
    /// The reference locator (the PURL string).
    pub reference_locator:  String,
}

/// A checksum entry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpdxChecksum
{
    /// Algorithm name (e.g. `SHA256`).
    pub algorithm:      String,
    /// Hex-encoded checksum value.
    pub checksum_value: String,
}

/// A relationship between two SPDX elements.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpdxRelationship
{
    /// Source element ID.
    pub spdx_element_id:      String,
    /// Relationship type (e.g. `DESCRIBES`, `CONTAINS`).
    pub relationship_type:    String,
    /// Target element ID.
    pub related_spdx_element: String,
}

impl SpdxDocument
{
    /// Serializes the document to pretty-printed JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json(&self) -> Result<String, serde_json::Error>
    {
        serde_json::to_string_pretty(self)
    }

    /// Serializes the document to compact JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_json_compact(&self) -> Result<String, serde_json::Error>
    {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    #[test]
    fn test_serialization()
    {
        let doc = SpdxDocument {
            spdx_version:       "SPDX-2.3".into(),
            data_license:       "CC0-1.0".into(),
            spdx_id:            "SPDXRef-DOCUMENT".into(),
            name:               "pk-sbom".into(),
            document_namespace: "https://spdx.org/spdxdocs/pk-0.2.0".into(),
            creation_info:      SpdxCreationInfo {
                created:              "2026-08-02T00:00:00Z".into(),
                creators:             vec!["Tool: pk-0.2.0".into()],
                license_list_version: Some("3.22".into()),
            },
            packages:           vec![SpdxPackage {
                spdx_id:           "SPDXRef-Package-curl".into(),
                name:              "curl".into(),
                version_info:      Some("7.81.0".into()),
                download_location: "NOASSERTION".into(),
                files_analyzed:    false,
                license_concluded: "NOASSERTION".into(),
                license_declared:  "NOASSERTION".into(),
                copyright_text:    "NOASSERTION".into(),
                description:       Some("command line tool".into()),
                external_refs:     vec![SpdxExternalRef {
                    reference_category: "PACKAGE-MANAGER".into(),
                    reference_type:     "purl".into(),
                    reference_locator:  "pkg:deb/debian/curl@7.81.0".into(),
                }],
                checksums:         None,
            }],
            relationships:      vec![SpdxRelationship {
                spdx_element_id:      "SPDXRef-DOCUMENT".into(),
                relationship_type:    "DESCRIBES".into(),
                related_spdx_element: "SPDXRef-Package-curl".into(),
            }],
        };

        let json = doc.to_json().unwrap();
        assert!(json.contains("\"spdxVersion\": \"SPDX-2.3\""));
        assert!(json.contains("pkg:deb/debian/curl@7.81.0"));
        assert!(json.contains("\"DESCRIBES\""));
    }
}
