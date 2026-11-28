// stream A: `effective(tx, scope, name)` and the `/v1/settings` routes land here.
//! The settings of docs/settings.md, typed: every name, its scopes, its values and its default.
//! A missing `settings` row means the default; nothing here is cached across requests.
use crate::channels::ChannelKind;
use crate::events;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Org,
    Agent,
    Space,
    Member,
}

/// Every setting of docs/settings.md; a test keeps the two equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Name {
    ChannelsEnabled,
    ChannelsAgent,
    ChannelsStreaming,
    MemberLinkedIdentities,
    ThreadsMultiplayer,
    SpacesGuestToolCalls,
    ApprovalsChannels,
    ApprovalsThreadCards,
}

impl Name {
    pub const ALL: [Name; 8] = [
        Name::ChannelsEnabled,
        Name::ChannelsAgent,
        Name::ChannelsStreaming,
        Name::MemberLinkedIdentities,
        Name::ThreadsMultiplayer,
        Name::SpacesGuestToolCalls,
        Name::ApprovalsChannels,
        Name::ApprovalsThreadCards,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Name::ChannelsEnabled => "channels.enabled",
            Name::ChannelsAgent => "channels.agent",
            Name::ChannelsStreaming => "channels.streaming",
            Name::MemberLinkedIdentities => "member.linked_identities",
            Name::ThreadsMultiplayer => "threads.multiplayer",
            Name::SpacesGuestToolCalls => "spaces.guest_tool_calls",
            Name::ApprovalsChannels => "approvals.channels",
            Name::ApprovalsThreadCards => "approvals.thread_cards",
        }
    }

    /// An unknown name is `None`, which the routes refuse (422).
    pub fn parse(s: &str) -> Option<Name> {
        Name::ALL.into_iter().find(|n| n.as_str() == s)
    }

    /// Where it may be set; the first scope is where its default applies.
    pub fn scopes(self) -> &'static [Scope] {
        match self {
            Name::ChannelsEnabled | Name::ChannelsAgent | Name::ChannelsStreaming => &[Scope::Org],
            Name::ThreadsMultiplayer | Name::SpacesGuestToolCalls | Name::ApprovalsThreadCards => {
                &[Scope::Org, Scope::Space]
            }
            Name::MemberLinkedIdentities | Name::ApprovalsChannels => &[Scope::Member],
        }
    }

    /// Changed through its own routes (installations, linked identities), never `/v1/settings`.
    pub fn own_routes(self) -> bool {
        matches!(
            self,
            Name::ChannelsEnabled | Name::ChannelsAgent | Name::MemberLinkedIdentities
        )
    }

    /// The `setting` of the `setting.changed.v1` a change publishes; `None` for those with
    /// their own routes and events.
    pub fn event(self) -> Option<events::Setting> {
        match self {
            Name::ChannelsStreaming => Some(events::Setting::ChannelStreaming),
            Name::ThreadsMultiplayer => Some(events::Setting::ThreadMultiplayer),
            Name::SpacesGuestToolCalls => Some(events::Setting::GuestToolCalls),
            Name::ApprovalsChannels => Some(events::Setting::ApprovalChannels),
            Name::ApprovalsThreadCards => Some(events::Setting::ThreadApprovalCards),
            Name::ChannelsEnabled | Name::ChannelsAgent | Name::MemberLinkedIdentities => None,
        }
    }
}

/// `channels.streaming`, per kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Streaming {
    On,
    Off,
}

impl Streaming {
    /// On where the provider can stream (Slack); off elsewhere.
    pub fn default_for(kind: ChannelKind) -> Streaming {
        match kind {
            ChannelKind::Slack => Streaming::On,
            ChannelKind::Web | ChannelKind::Whatsapp | ChannelKind::Sms => Streaming::Off,
        }
    }
}

/// `threads.multiplayer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Multiplayer {
    #[default]
    Join,
    StarterOnly,
}

/// `spaces.guest_tool_calls`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuestToolCalls {
    #[default]
    Ask,
    Refuse,
}

/// One entry of `approvals.channels`; its default (every linked channel, then `email`) depends
/// on the member's links, so it is computed where it is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalChannel {
    Slack,
    Whatsapp,
    Sms,
    Email,
}

/// `approvals.thread_cards`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadCards {
    Off,
    #[default]
    Status,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The settings named in the tables of docs/settings.md, in order.
    fn documented() -> Vec<String> {
        let doc = include_str!("../../docs/settings.md");
        let start = doc.find("## Channels").unwrap();
        doc[start..]
            .lines()
            .filter_map(|l| l.strip_prefix("| `")?.split('`').next())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn the_doc_and_the_enum_list_the_same_settings() {
        let typed: Vec<String> = Name::ALL.iter().map(|n| n.as_str().to_owned()).collect();
        assert_eq!(
            documented(),
            typed,
            "docs/settings.md and settings::Name disagree"
        );
        for n in Name::ALL {
            assert_eq!(Name::parse(n.as_str()), Some(n));
        }
        assert_eq!(Name::parse("channels.unknown"), None);
    }

    #[test]
    fn defaults_are_the_documented_safe_choices() {
        assert_eq!(Streaming::default_for(ChannelKind::Slack), Streaming::On);
        assert_eq!(
            Streaming::default_for(ChannelKind::Whatsapp),
            Streaming::Off
        );
        assert_eq!(Streaming::default_for(ChannelKind::Sms), Streaming::Off);
        assert_eq!(Multiplayer::default(), Multiplayer::Join);
        assert_eq!(GuestToolCalls::default(), GuestToolCalls::Ask);
        assert_eq!(ThreadCards::default(), ThreadCards::Status);
    }

    #[test]
    fn every_setting_changed_here_has_an_event_and_the_rest_have_their_own_routes() {
        for n in Name::ALL {
            assert_eq!(n.event().is_none(), n.own_routes(), "{n:?}");
        }
    }

    #[test]
    fn unknown_values_are_refused() {
        assert!(serde_json::from_str::<Multiplayer>("\"starter_only\"").is_ok());
        for bad in ["\"everyone\"", "\"JOIN\"", "1", "null"] {
            assert!(serde_json::from_str::<Multiplayer>(bad).is_err(), "{bad}");
            assert!(
                serde_json::from_str::<GuestToolCalls>(bad).is_err(),
                "{bad}"
            );
        }
        assert!(serde_json::from_str::<Vec<ApprovalChannel>>(r#"["slack","web"]"#).is_err());
    }
}
