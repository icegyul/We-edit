//! Stable opaque identities (DM-002). IDs are never memory addresses or array positions; they are
//! allocated from a per-project counter so saved projects and tests are deterministic.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($(#[$doc:meta] $name:ident = $prefix:literal;)*) => {$(
        #[$doc]
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Debug::fmt(self, f)
            }
        }
    )*};
}

id_type! {
    /// A project-panel entry: bin, media clip, sequence, synthetic item.
    ItemId = "item";
    /// A media file known to the project.
    AssetId = "asset";
    /// A sequence (timeline).
    SequenceId = "seq";
    /// A track inside a sequence.
    TrackId = "track";
    /// A clip on a track.
    ClipId = "clip";
    /// A transition between (or at the edge of) clips.
    TransitionId = "tr";
    /// An effect instance on a clip or track.
    ComponentId = "fx";
    /// A marker.
    MarkerId = "mk";
    /// Relation that ties the video and audio parts of one source together (DM-TL-003).
    LinkId = "link";
    /// A user group of clips (DM-TL-003: independent from links).
    GroupId = "grp";
}

/// Allocates identities. Stored in the project so new IDs never collide with saved ones.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdGen {
    next: u64,
}

impl Default for IdGen {
    fn default() -> Self {
        IdGen { next: 1 }
    }
}

impl IdGen {
    pub fn next_raw(&mut self) -> u64 {
        let v = self.next;
        self.next += 1;
        v
    }

    /// Makes sure future IDs are above `seen` (after loading or merging data).
    pub fn observe(&mut self, seen: u64) {
        if seen >= self.next {
            self.next = seen + 1;
        }
    }

    pub fn item(&mut self) -> ItemId {
        ItemId(self.next_raw())
    }
    pub fn asset(&mut self) -> AssetId {
        AssetId(self.next_raw())
    }
    pub fn sequence(&mut self) -> SequenceId {
        SequenceId(self.next_raw())
    }
    pub fn track(&mut self) -> TrackId {
        TrackId(self.next_raw())
    }
    pub fn clip(&mut self) -> ClipId {
        ClipId(self.next_raw())
    }
    pub fn transition(&mut self) -> TransitionId {
        TransitionId(self.next_raw())
    }
    pub fn component(&mut self) -> ComponentId {
        ComponentId(self.next_raw())
    }
    pub fn marker(&mut self) -> MarkerId {
        MarkerId(self.next_raw())
    }
    pub fn link(&mut self) -> LinkId {
        LinkId(self.next_raw())
    }
    pub fn group(&mut self) -> GroupId {
        GroupId(self.next_raw())
    }
}
