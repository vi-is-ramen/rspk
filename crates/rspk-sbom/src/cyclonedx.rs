//! CycloneDX 1.6 BOM model.
//!
//! Implements the subset of the CycloneDX specification needed for
//! a complete system-level SBOM. Serializes to JSON via serde.

use serde::Serialize;

/// Top-level CycloneDX BOM document.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CycloneDxBom
{
    /// Always `"CycloneDX"`.
    pub bom_format:    String,
    /// Specification version.
    pub spec_version:  String,
    /// Unique BOM identifier (`urn:uuid:...`).
    pub serial_number: String,
    /// BOM revision (starts at 1).
    pub version:       u32,
    /// Document metadata.
    pub metadata:      CdxMetadata,
    /// Software components.
    pub components:    Vec<CdxComponent>,
}

/// BOM metadata block.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdxMetadata
{
    /// ISO 8601 timestamp of BOM creation.
    pub timestamp: String,
    /// Tools that generated the BOM.
    pub tools:     Vec<CdxTool>,
    /// The component this BOM describes (the host system).
    pub component: CdxComponent,
}

/// A tool that participated in BOM generation.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdxTool
{
    /// Tool vendor.
    pub vendor:  String,
    /// Tool name.
    pub name:    String,
    /// Tool version.
    pub version: String,
}

/// A software component in the BOM.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdxComponent
{
    /// Component type (`application`, `library`, `operating-system`,
    /// `framework`).
    #[serde(rename = "type")]
    pub component_type: String,
    /// Unique reference within this BOM (the PURL).
    pub bom_ref:        String,
    /// Component name.
    pub name:           String,
    /// Component version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version:        Option<String>,
    /// Package URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purl:           Option<String>,
    /// Component description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description:    Option<String>,
    /// Additional key-value properties.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub properties:     Vec<CdxProperty>,
}

/// A key-value property attached to a component.
#[derive(Debug, Clone, Serialize)]
pub struct CdxProperty
{
    /// Property name.
    pub name:  String,
    /// Property value.
    pub value: String,
}

impl CycloneDxBom
{
    /// Serializes the BOM to pretty-printed JSON.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails (should not happen
    /// with valid data).
    pub fn to_json(&self) -> Result<String, serde_json::Error>
    {
        serde_json::to_string_pretty(self)
    }

    /// Serializes the BOM to compact JSON (single line).
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
        let bom = CycloneDxBom {
            bom_format:    "CycloneDX".into(),
            spec_version:  "1.6".into(),
            serial_number: "urn:uuid:test".into(),
            version:       1,
            metadata:      CdxMetadata {
                timestamp: "2026-08-02T00:00:00Z".into(),
                tools:     vec![CdxTool {
                    vendor:  "rspk".into(),
                    name:    "pk".into(),
                    version: "0.2.0".into(),
                }],
                component: CdxComponent {
                    component_type: "application".into(),
                    bom_ref:        "host".into(),
                    name:           "host-system".into(),
                    version:        None,
                    purl:           None,
                    description:    None,
                    properties:     vec![],
                },
            },
            components:    vec![CdxComponent {
                component_type: "library".into(),
                bom_ref:        "pkg:deb/debian/curl@7.81.0".into(),
                name:           "curl".into(),
                version:        Some("7.81.0".into()),
                purl:           Some("pkg:deb/debian/curl@7.81.0".into()),
                description:    Some("command line tool".into()),
                properties:     vec![CdxProperty {
                    name:  "pk:manager".into(),
                    value: "apt".into(),
                }],
            }],
        };

        let json = bom.to_json().unwrap();
        assert!(json.contains("\"bomFormat\": \"CycloneDX\""));
        assert!(json.contains("\"specVersion\": \"1.6\""));
        assert!(json.contains("pkg:deb/debian/curl@7.81.0"));
        assert!(json.contains("\"pk:manager\""));
    }
}
