#[allow(dead_code)]
pub mod v22 {
    use crate::dev::event::BadgeRef;
    use crate::dev::host::abi_2_2::modus;

    fn badges_from_wit(msg: &modus::abi::types::Message) -> Vec<BadgeRef> {
        crate::dev::event::sanitize_badges(
            msg.badges
                .iter()
                .map(|b| BadgeRef {
                    set: b.set.clone(),
                    version: b.version.clone(),
                })
                .collect(),
        )
    }

    fn message_to_wit(
        user_id: String,
        display_name: String,
        fragments: Vec<modus::abi::types::Fragment>,
        name_color: Option<String>,
        message_id: Option<String>,
        rewarded: bool,
        badges: Vec<BadgeRef>,
    ) -> modus::abi::types::Message {
        modus::abi::types::Message {
            user_id,
            display_name,
            fragments,
            name_color,
            message_id,
            rewarded,
            badges: badges
                .into_iter()
                .map(|b| modus::abi::types::BadgeRef {
                    set: b.set,
                    version: b.version,
                })
                .collect(),
        }
    }

    include!("wit_map_body.rs");
}

#[allow(dead_code)]
pub mod v21 {
    use crate::dev::event::BadgeRef;
    use crate::dev::host::abi_2_1::modus;

    fn badges_from_wit(_msg: &modus::abi::types::Message) -> Vec<BadgeRef> {
        Vec::new()
    }

    fn message_to_wit(
        user_id: String,
        display_name: String,
        fragments: Vec<modus::abi::types::Fragment>,
        name_color: Option<String>,
        message_id: Option<String>,
        rewarded: bool,
        _badges: Vec<BadgeRef>,
    ) -> modus::abi::types::Message {
        modus::abi::types::Message {
            user_id,
            display_name,
            fragments,
            name_color,
            message_id,
            rewarded,
        }
    }

    include!("wit_map_body.rs");
}

pub(crate) use v22::*;
