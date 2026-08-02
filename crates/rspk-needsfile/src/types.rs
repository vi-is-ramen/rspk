//! Core types for Needsfile AST and evaluation context.

use rspk_core::Platform;
use std::collections::HashSet;

/// A single entry from a Needsfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeedsEntry
{
    /// The package name.
    pub package: String,
    /// Optional package manager to use.
    pub manager: Option<String>,
    /// Optional version constraint.
    pub version: Option<String>,
}

impl NeedsEntry
{
    /// Creates a new needs entry.
    pub fn new(
        package: impl Into<String>,
        manager: Option<String>,
        version: Option<String>,
    ) -> Self
    {
        Self {
            package: package.into(),
            manager,
            version,
        }
    }
}

/// An item inside a Needsfile: either an unconditional entry or a
/// conditional block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeedsItem
{
    /// A plain package entry.
    Entry(NeedsEntry),
    /// A conditional block: `if <condition> { ... }`.
    Conditional(ConditionalBlock),
}

/// A conditional block containing a boolean condition and a list of
/// nested items (which may themselves be conditional).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionalBlock
{
    /// The condition guarding this block.
    pub condition: Condition,
    /// Items inside the block.
    pub items:     Vec<NeedsItem>,
}

/// A boolean condition used to gate conditional blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition
{
    /// `os = <name>`
    OsEq(String),
    /// `present "<manager>"`
    ManagerPresent(String),
    /// `feature "<name>"`
    FeaturePresent(String),
    /// `mode = "<value>"`
    ModeEq(String),
    /// `!<inner>`
    Not(Box<Condition>),
    /// `<left> && <right>`
    And(Box<Condition>, Box<Condition>),
    /// `<left> || <right>`
    Or(Box<Condition>, Box<Condition>),
}

/// Context used to evaluate conditions.
///
/// Carries everything a condition may need: the current platform,
/// the set of available managers, enabled features and the active
/// mode.
#[derive(Debug, Clone, Default)]
pub struct EvalContext
{
    /// Current operating system.
    pub os:                 Option<Platform>,
    /// Set of available package manager IDs (e.g. `"apt"`, `"cargo"`).
    pub available_managers: HashSet<String>,
    /// Enabled feature flags.
    pub features:           HashSet<String>,
    /// Current mode (e.g. `"dev"`, `"prod"`).
    pub mode:               Option<String>,
}

impl EvalContext
{
    /// Creates a new context for the current platform with no
    /// managers, features or mode.
    pub fn for_current_platform() -> Self
    {
        Self {
            os:                 Platform::current().ok(),
            available_managers: HashSet::new(),
            features:           HashSet::new(),
            mode:               None,
        }
    }

    /// Adds a manager ID to the available set.
    pub fn with_manager(mut self, id: impl Into<String>) -> Self
    {
        self.available_managers.insert(id.into());
        self
    }

    /// Enables a feature.
    pub fn with_feature(mut self, name: impl Into<String>) -> Self
    {
        self.features.insert(name.into());
        self
    }

    /// Sets the active mode.
    pub fn with_mode(mut self, mode: impl Into<String>) -> Self
    {
        self.mode = Some(mode.into());
        self
    }
}
