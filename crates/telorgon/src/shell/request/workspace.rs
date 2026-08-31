//! Typed workspace-policy intentions emitted by shell UI.

use crate::shell::{
    InputSource, ShellCapabilities, SurfaceId, WorkspaceId, WorkspaceName, WorkspaceRevision,
};

/// A request against host-owned workspace order and membership.
///
/// Every existing workspace cited by a mutation carries the exact revision observed by the shell.
/// The host remains responsible for rejecting stale revisions and deciding focus, layout,
/// membership, creation, and removal policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceRequest {
    Select {
        workspace: WorkspaceId,
        revision: WorkspaceRevision,
        source: InputSource,
    },
    MoveSurface {
        surface: SurfaceId,
        from: WorkspaceId,
        from_revision: WorkspaceRevision,
        to: WorkspaceId,
        to_revision: WorkspaceRevision,
    },
    Reorder {
        workspace: WorkspaceId,
        revision: WorkspaceRevision,
        order: u32,
    },
    Create {
        name: WorkspaceName,
        order: u32,
    },
    Remove {
        workspace: WorkspaceId,
        revision: WorkspaceRevision,
    },
}

impl WorkspaceRequest {
    pub const fn required_capability(&self) -> ShellCapabilities {
        match self {
            Self::Select { .. } => ShellCapabilities::SELECT_WORKSPACE,
            Self::MoveSurface { .. }
            | Self::Reorder { .. }
            | Self::Create { .. }
            | Self::Remove { .. } => ShellCapabilities::MANAGE_WORKSPACES,
        }
    }

    pub const fn observed_workspace(&self) -> Option<(WorkspaceId, WorkspaceRevision)> {
        match self {
            Self::Select {
                workspace,
                revision,
                ..
            }
            | Self::Reorder {
                workspace,
                revision,
                ..
            }
            | Self::Remove {
                workspace,
                revision,
            } => Some((*workspace, *revision)),
            Self::MoveSurface {
                from,
                from_revision,
                ..
            } => Some((*from, *from_revision)),
            Self::Create { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(value: u64) -> WorkspaceId {
        WorkspaceId::from_raw(value).unwrap()
    }

    fn revision(value: u64) -> WorkspaceRevision {
        WorkspaceRevision::from_raw(value).unwrap()
    }

    #[test]
    fn selection_and_management_have_distinct_authority() {
        let select = WorkspaceRequest::Select {
            workspace: workspace(1),
            revision: revision(4),
            source: InputSource::Keyboard,
        };
        let reorder = WorkspaceRequest::Reorder {
            workspace: workspace(1),
            revision: revision(4),
            order: 3,
        };

        assert_eq!(
            select.required_capability(),
            ShellCapabilities::SELECT_WORKSPACE
        );
        assert_eq!(
            reorder.required_capability(),
            ShellCapabilities::MANAGE_WORKSPACES
        );
        assert_eq!(
            reorder.observed_workspace(),
            Some((workspace(1), revision(4)))
        );
    }

    #[test]
    fn cross_workspace_move_retains_both_observed_revisions() {
        let request = WorkspaceRequest::MoveSurface {
            surface: SurfaceId::from_raw(8).unwrap(),
            from: workspace(1),
            from_revision: revision(10),
            to: workspace(2),
            to_revision: revision(20),
        };

        assert_eq!(
            request.observed_workspace(),
            Some((workspace(1), revision(10)))
        );
        assert!(matches!(
            request,
            WorkspaceRequest::MoveSurface {
                to,
                to_revision,
                ..
            } if to == workspace(2) && to_revision == revision(20)
        ));
    }

    #[test]
    fn creation_retains_validated_name_and_requested_order_without_fabricating_identity() {
        let request = WorkspaceRequest::Create {
            name: WorkspaceName::new("Writing").unwrap(),
            order: 2,
        };

        assert_eq!(request.observed_workspace(), None);
        assert!(matches!(
            request,
            WorkspaceRequest::Create { ref name, order }
                if name.as_str() == "Writing" && order == 2
        ));
    }
}
