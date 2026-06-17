use chrono::{NaiveDate, NaiveTime, Utc};

use crate::core::model::{Author, Contact, Message};

/// Demo roster used to bootstrap the chat until contacts are persisted.
pub fn roster() -> Vec<Contact> {
    let today = Utc::now().date_naive();
    let yesterday = today.pred_opt().unwrap_or(today);

    let at = |date: NaiveDate, h: u32, m: u32| {
        date.and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap())
            .and_utc()
    };

    vec![
        Contact {
            name: "alice".into(),
            unread: 0,
            last_message: "nice, ttyl".into(),
            history: vec![
                Message {
                    author: Author::Them,
                    timestamp: at(yesterday, 10, 30),
                    body: "hey, are you on the new repeater?".into(),
                },
                Message {
                    author: Author::Me,
                    timestamp: at(yesterday, 10, 31),
                    body: "yep, just hopped on. signal's clean".into(),
                },
                Message {
                    author: Author::Them,
                    timestamp: at(yesterday, 10, 32),
                    body: "how's the mesh holding up?".into(),
                },
                Message {
                    author: Author::Me,
                    timestamp: at(yesterday, 10, 33),
                    body: "solid".into(),
                },
                Message {
                    author: Author::Me,
                    timestamp: at(yesterday, 10, 33),
                    body: "way better than the old node".into(),
                },
                Message {
                    author: Author::Me,
                    timestamp: at(yesterday, 10, 34),
                    body: "we should drop a third repeater by the ridge".into(),
                },
                Message {
                    author: Author::Them,
                    timestamp: at(yesterday, 10, 36),
                    body: "agreed".into(),
                },
                Message {
                    author: Author::Them,
                    timestamp: at(yesterday, 10, 36),
                    body: "i'll bring the hardware tomorrow".into(),
                },
                Message {
                    author: Author::Them,
                    timestamp: at(today, 9, 15),
                    body: "did you get the hardware set up?".into(),
                },
                Message {
                    author: Author::Me,
                    timestamp: at(today, 9, 17),
                    body: "working on it now".into(),
                },
                Message {
                    author: Author::Me,
                    timestamp: at(today, 9, 17),
                    body: "connector was the wrong gauge but i rigged it".into(),
                },
                Message {
                    author: Author::Them,
                    timestamp: at(today, 9, 20),
                    body: "nice, ttyl".into(),
                },
            ],
        },
        Contact {
            name: "bob".into(),
            unread: 0,
            last_message: "ttyl".into(),
            history: vec![Message {
                author: Author::Them,
                timestamp: at(yesterday, 9, 58),
                body: "heading off-grid, ttyl".into(),
            }],
        },
        Contact {
            name: "carol".into(),
            unread: 5,
            last_message: "ping me when you see this".into(),
            history: vec![Message {
                author: Author::Them,
                timestamp: at(today, 10, 40),
                body: "ping me when you see this".into(),
            }],
        },
        Contact {
            name: "dave".into(),
            unread: 2,
            last_message: "relay node is up".into(),
            history: vec![Message {
                author: Author::Them,
                timestamp: at(today, 8, 12),
                body: "relay node is up on channel 7".into(),
            }],
        },
    ]
}
