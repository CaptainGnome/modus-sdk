pub fn sanitize_name_color(raw: Option<&str>) -> Option<String> {
    let value = raw?.trim();
    let parts: Vec<&str> = value.split('>').map(str::trim).filter(|p| !p.is_empty()).collect();
    if !(1..=4).contains(&parts.len()) {
        return None;
    }
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if part.len() != 7
            || !part.starts_with('#')
            || !part.as_bytes()[1..].iter().all(|b| b.is_ascii_hexdigit())
        {
            return None;
        }
        if i > 0 {
            out.push('>');
        }
        out.push_str(part);
    }
    Some(out)
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
        assert_eq!(
            sanitize_name_color(Some("#EC8C24>#F2F932")).as_deref(),
            Some("#EC8C24>#F2F932")
        );
        assert_eq!(
            sanitize_name_color(Some("#111111>#222222>#333333>#444444")).as_deref(),
            Some("#111111>#222222>#333333>#444444")
        );
        assert!(sanitize_name_color(Some("red")).is_none());
        assert!(sanitize_name_color(Some("#fff")).is_none());
        assert!(sanitize_name_color(None).is_none());
        assert!(sanitize_name_color(Some("#GG0000")).is_none());
        assert!(sanitize_name_color(Some("#111111>#222222>#333333>#444444>#555555")).is_none());
    }
}
