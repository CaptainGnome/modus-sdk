use super::event::{sanitize_name_color, Fragment, ModAction, Money, Payload, SystemCode, SystemEvent};
use super::host::modus;

pub(crate) fn empty_to_none(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.is_empty())
}

fn map_fragment(fragment: modus::abi::types::Fragment) -> Fragment {
    match fragment {
        modus::abi::types::Fragment::Text(text) => Fragment::Text { text },
        modus::abi::types::Fragment::Emote(emote) => Fragment::Emote {
            id: emote.id,
            alt: emote.alt,
            url: emote.url,
        },
        modus::abi::types::Fragment::Mention(mention) => Fragment::Mention {
            user_id: mention.user_id,
            display_name: mention.display_name,
        },
        modus::abi::types::Fragment::Url(href) => Fragment::Url { href },
    }
}

pub(crate) fn map_payload(payload: modus::abi::types::Payload) -> Result<Payload, String> {
    use modus::abi::types::Payload as WitPayload;
    Ok(match payload {
        WitPayload::Message(msg) => Payload::Message {
            user_id: msg.user_id,
            display_name: msg.display_name,
            fragments: msg.fragments.into_iter().map(map_fragment).collect(),
            name_color: sanitize_name_color(msg.name_color),
            message_id: empty_to_none(msg.message_id),
            rewarded: msg.rewarded,
        },
        WitPayload::Donation(don) => Payload::Donation {
            user_id: don.user_id,
            display_name: don.display_name,
            money: Money {
                amount: don.money.amount,
                currency: don.money.currency,
            },
            fragments: don.fragments.into_iter().map(map_fragment).collect(),
        },
        WitPayload::Sub(sub) => Payload::Sub {
            user_id: sub.user_id,
            display_name: sub.display_name,
            months: sub.months,
            tier: sub.tier,
            gifted: sub.gifted,
            gifter_id: sub.gifter_id,
            gifter_name: sub.gifter_name,
            fragments: sub.fragments.into_iter().map(map_fragment).collect(),
        },
        WitPayload::Follow(follow) => Payload::Follow {
            user_id: follow.user_id,
            display_name: follow.display_name,
        },
        WitPayload::Raid(raid) => Payload::Raid {
            from_user_id: raid.from_user_id,
            from_display_name: raid.from_display_name,
            viewers: raid.viewers,
        },
        WitPayload::ViewerCount(item) => Payload::ViewerCount {
            count: item.count,
        },
        WitPayload::Reward(item) => Payload::Reward {
            user_id: item.user_id,
            display_name: item.display_name,
            reward_id: item.reward_id,
            title: item.title,
            cost: item.cost,
            fragments: item.fragments.into_iter().map(map_fragment).collect(),
            image_url: empty_to_none(item.image_url),
        },
        WitPayload::Moderation(item) => Payload::Moderation {
            action: map_mod_action(item.action),
            target_user_id: item.target_user_id,
            target_display_name: item.target_display_name,
            moderator_id: item.moderator_id,
            moderator_name: item.moderator_name,
            message_id: item.message_id,
            duration_sec: item.duration_sec,
        },
        WitPayload::System(ev) => Payload::System(from_wit_system(ev)),
        WitPayload::Custom(custom) => Payload::Custom {
            kind: custom.kind,
            fields: custom.fields,
        },
    })
}

fn from_wit_system(ev: modus::abi::types::SystemEvent) -> SystemEvent {
    SystemEvent {
        code: from_wit_system_code(ev.code),
        plugin_id: ev.plugin_id,
        account_id: ev.account_id,
        platform: ev.platform,
        detail: ev.detail,
    }
}

fn from_wit_system_code(code: modus::abi::types::SystemCode) -> SystemCode {
    match code {
        modus::abi::types::SystemCode::PluginDisabled => SystemCode::PluginDisabled,
        modus::abi::types::SystemCode::PluginCrashed => SystemCode::PluginCrashed,
        modus::abi::types::SystemCode::PluginQuarantined => SystemCode::PluginQuarantined,
        modus::abi::types::SystemCode::PluginRollback => SystemCode::PluginRollback,
        modus::abi::types::SystemCode::PluginReconnecting => SystemCode::PluginReconnecting,
        modus::abi::types::SystemCode::PluginLoadFailed => SystemCode::PluginLoadFailed,
        modus::abi::types::SystemCode::PluginRemoved => SystemCode::PluginRemoved,
        modus::abi::types::SystemCode::AuthConnected => SystemCode::AuthConnected,
        modus::abi::types::SystemCode::AuthDisconnected => SystemCode::AuthDisconnected,
        modus::abi::types::SystemCode::AuthRevoked => SystemCode::AuthRevoked,
        modus::abi::types::SystemCode::AuthLoginFailed => SystemCode::AuthLoginFailed,
        modus::abi::types::SystemCode::NetworkResume => SystemCode::NetworkResume,
        modus::abi::types::SystemCode::WsClosed => SystemCode::WsClosed,
        modus::abi::types::SystemCode::Unknown => SystemCode::Unknown,
    }
}

fn map_mod_action(action: modus::abi::types::ModAction) -> ModAction {
    match action {
        modus::abi::types::ModAction::Delete => ModAction::Delete,
        modus::abi::types::ModAction::Timeout => ModAction::Timeout,
        modus::abi::types::ModAction::Ban => ModAction::Ban,
        modus::abi::types::ModAction::Unban => ModAction::Unban,
    }
}

pub(crate) fn to_wit_event(event: super::event::Event) -> modus::abi::wait::Event {
    modus::abi::wait::Event {
        id: event.id.to_string(),
        ts: event.ts,
        source: modus::abi::wait::Source {
            plugin_id: event.source.plugin_id,
            platform: event.source.platform,
            channel: event.source.channel,
        },
        payload: to_wit_payload(event.payload),
        opaque: event
            .opaque
            .as_ref()
            .and_then(|value| serde_json::to_string(value).ok()),
        flags: modus::abi::wait::FilterFlags {
            hide_chat: event.flags.hide_chat,
            skip_alert: event.flags.skip_alert,
            highlight: event.flags.highlight,
            mask: event.flags.mask,
        },
    }
}

fn to_wit_payload(payload: Payload) -> modus::abi::types::Payload {
    use modus::abi::types::Payload as WitPayload;
    match payload {
        Payload::Message {
            user_id,
            display_name,
            fragments,
            name_color,
            message_id,
            rewarded,
        } => WitPayload::Message(modus::abi::types::Message {
            user_id,
            display_name,
            fragments: fragments.into_iter().map(to_wit_fragment).collect(),
            name_color,
            message_id,
            rewarded,
        }),
        Payload::Donation {
            user_id,
            display_name,
            money,
            fragments,
        } => WitPayload::Donation(modus::abi::types::Donation {
            user_id,
            display_name,
            money: modus::abi::types::Money {
                amount: money.amount,
                currency: money.currency,
            },
            fragments: fragments.into_iter().map(to_wit_fragment).collect(),
        }),
        Payload::Sub {
            user_id,
            display_name,
            months,
            tier,
            gifted,
            gifter_id,
            gifter_name,
            fragments,
        } => WitPayload::Sub(modus::abi::types::Sub {
            user_id,
            display_name,
            months,
            tier,
            gifted,
            gifter_id,
            gifter_name,
            fragments: fragments.into_iter().map(to_wit_fragment).collect(),
        }),
        Payload::Follow {
            user_id,
            display_name,
        } => WitPayload::Follow(modus::abi::types::Follow {
            user_id,
            display_name,
        }),
        Payload::Raid {
            from_user_id,
            from_display_name,
            viewers,
        } => WitPayload::Raid(modus::abi::types::Raid {
            from_user_id,
            from_display_name,
            viewers,
        }),
        Payload::ViewerCount { count } => {
            WitPayload::ViewerCount(modus::abi::types::ViewerCount { count })
        }
        Payload::Reward {
            user_id,
            display_name,
            reward_id,
            title,
            cost,
            fragments,
            image_url,
        } => WitPayload::Reward(modus::abi::types::Reward {
            user_id,
            display_name,
            reward_id,
            title,
            cost,
            fragments: fragments.into_iter().map(to_wit_fragment).collect(),
            image_url,
        }),
        Payload::Moderation {
            action,
            target_user_id,
            target_display_name,
            moderator_id,
            moderator_name,
            message_id,
            duration_sec,
        } => WitPayload::Moderation(modus::abi::types::Moderation {
            action: to_wit_mod_action(action),
            target_user_id,
            target_display_name,
            moderator_id,
            moderator_name,
            message_id,
            duration_sec,
        }),
        Payload::System(ev) => WitPayload::System(to_wit_system(ev)),
        Payload::Custom { kind, fields } => {
            WitPayload::Custom(modus::abi::types::CustomEvent { kind, fields })
        }
    }
}

fn to_wit_system(ev: SystemEvent) -> modus::abi::types::SystemEvent {
    modus::abi::types::SystemEvent {
        code: to_wit_system_code(ev.code),
        plugin_id: ev.plugin_id,
        account_id: ev.account_id,
        platform: ev.platform,
        detail: ev.detail,
    }
}

fn to_wit_system_code(code: SystemCode) -> modus::abi::types::SystemCode {
    match code {
        SystemCode::PluginDisabled => modus::abi::types::SystemCode::PluginDisabled,
        SystemCode::PluginCrashed => modus::abi::types::SystemCode::PluginCrashed,
        SystemCode::PluginQuarantined => modus::abi::types::SystemCode::PluginQuarantined,
        SystemCode::PluginRollback => modus::abi::types::SystemCode::PluginRollback,
        SystemCode::PluginReconnecting => modus::abi::types::SystemCode::PluginReconnecting,
        SystemCode::PluginLoadFailed => modus::abi::types::SystemCode::PluginLoadFailed,
        SystemCode::PluginRemoved => modus::abi::types::SystemCode::PluginRemoved,
        SystemCode::AuthConnected => modus::abi::types::SystemCode::AuthConnected,
        SystemCode::AuthDisconnected => modus::abi::types::SystemCode::AuthDisconnected,
        SystemCode::AuthRevoked => modus::abi::types::SystemCode::AuthRevoked,
        SystemCode::AuthLoginFailed => modus::abi::types::SystemCode::AuthLoginFailed,
        SystemCode::NetworkResume => modus::abi::types::SystemCode::NetworkResume,
        SystemCode::WsClosed => modus::abi::types::SystemCode::WsClosed,
        SystemCode::Unknown => modus::abi::types::SystemCode::Unknown,
    }
}

fn to_wit_fragment(fragment: Fragment) -> modus::abi::types::Fragment {
    match fragment {
        Fragment::Text { text } => modus::abi::types::Fragment::Text(text),
        Fragment::Emote { id, alt, url } => {
            modus::abi::types::Fragment::Emote(modus::abi::types::Emote { id, alt, url })
        }
        Fragment::Mention {
            user_id,
            display_name,
        } => modus::abi::types::Fragment::Mention(modus::abi::types::Mention {
            user_id,
            display_name,
        }),
        Fragment::Url { href } => modus::abi::types::Fragment::Url(href),
    }
}

fn to_wit_mod_action(action: ModAction) -> modus::abi::types::ModAction {
    match action {
        ModAction::Delete => modus::abi::types::ModAction::Delete,
        ModAction::Timeout => modus::abi::types::ModAction::Timeout,
        ModAction::Ban => modus::abi::types::ModAction::Ban,
        ModAction::Unban => modus::abi::types::ModAction::Unban,
    }
}
