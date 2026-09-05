pub fn sanitize_name_color(raw: Option<&str>) -> Option<String> {
    let value = raw?.trim();
    if value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(|b| b.is_ascii_hexdigit())
    {
        Some(value.to_string())
    } else {
        None
    }
}

#[cfg(any(feature = "consumer", feature = "emitter", feature = "connector"))]
mod payloads {
    use super::sanitize_name_color;
    use crate::types::{Donation, Follow, Fragment, Message, Money, Payload, Reward, ViewerCount};

    pub fn text_fragment(text: impl Into<String>) -> Fragment {
        Fragment::Text(text.into())
    }

    pub fn money(amount: f64, currency: impl Into<String>) -> Money {
        Money {
            amount,
            currency: currency.into(),
        }
    }

    pub fn text_message(
        user_id: impl Into<String>,
        display_name: impl Into<String>,
        text: impl Into<String>,
        message_id: Option<String>,
        name_color: Option<&str>,
    ) -> Payload {
        Payload::Message(Message {
            user_id: user_id.into(),
            display_name: display_name.into(),
            fragments: vec![text_fragment(text)],
            name_color: sanitize_name_color(name_color),
            message_id,
            rewarded: false,
        })
    }

    pub fn donation(
        user_id: impl Into<String>,
        display_name: impl Into<String>,
        amount: f64,
        currency: impl Into<String>,
        fragments: Vec<Fragment>,
    ) -> Payload {
        Payload::Donation(Donation {
            user_id: user_id.into(),
            display_name: display_name.into(),
            money: money(amount, currency),
            fragments,
        })
    }

    pub fn follow(
        user_id: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Payload {
        Payload::Follow(Follow {
            user_id: user_id.into(),
            display_name: display_name.into(),
        })
    }

    pub fn reward(
        user_id: impl Into<String>,
        display_name: impl Into<String>,
        reward_id: impl Into<String>,
        title: impl Into<String>,
        cost: u32,
        fragments: Vec<Fragment>,
        image_url: Option<String>,
    ) -> Payload {
        Payload::Reward(Reward {
            user_id: user_id.into(),
            display_name: display_name.into(),
            reward_id: reward_id.into(),
            title: title.into(),
            cost,
            fragments,
            image_url,
        })
    }

    pub fn viewer_count(count: u32) -> Payload {
        Payload::ViewerCount(ViewerCount { count })
    }
}

#[cfg(any(feature = "consumer", feature = "emitter", feature = "connector"))]
pub use payloads::{donation, follow, money, reward, text_fragment, text_message, viewer_count};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_hash_hex() {
        assert_eq!(
            sanitize_name_color(Some(" #Ff4500 ")).as_deref(),
            Some("#Ff4500")
        );
        assert!(sanitize_name_color(Some("red")).is_none());
        assert!(sanitize_name_color(Some("#fff")).is_none());
        assert!(sanitize_name_color(None).is_none());
        assert!(sanitize_name_color(Some("#GG0000")).is_none());
    }
}
