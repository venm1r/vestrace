//! Upgrade-only adoption lifecycle; terminal facts cannot be reopened.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LegacyAdoptionState {
    Planned,
    Rebuilding,
    ReadyToCutover,
    Completed,
    Failed,
}

impl LegacyAdoptionState {
    pub const ALL: [Self; 5] = [
        Self::Planned,
        Self::Rebuilding,
        Self::ReadyToCutover,
        Self::Completed,
        Self::Failed,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Rebuilding => "rebuilding",
            Self::ReadyToCutover => "ready_to_cutover",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

impl LegacyAdoptionState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
    pub const fn may_advance_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Planned, Self::Rebuilding | Self::Failed)
                | (Self::Rebuilding, Self::ReadyToCutover | Self::Failed)
                | (Self::ReadyToCutover, Self::Completed | Self::Failed)
        )
    }
}
