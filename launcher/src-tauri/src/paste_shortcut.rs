//! Ctrl+V delivery policy, independent of the platform input adapter.
const CTRL: u16 = 0x11;
const V: u16 = 0x56;

/// `send` inserts an ordered batch and returns the number of accepted events.
/// Each tuple is (virtual key, key-up). Never retry the paste itself.
pub fn send_paste(mut send: impl FnMut(&[(u16, bool)]) -> usize) -> Result<(), String> {
    let keys = [(CTRL, false), (V, false), (V, true), (CTRL, true)];
    let inserted = send(&keys);
    if inserted == keys.len() {
        return Ok(());
    }

    // Release only keys left down by the accepted prefix. A rejected batch
    // must not release the user's keys; a partial batch must not repeat Ctrl+V.
    let cleanup: &[(u16, bool)] = match inserted {
        1 | 3 => &[(CTRL, true)],
        2 => &[(V, true), (CTRL, true)],
        _ => &[],
    };
    if !cleanup.is_empty() && send(cleanup) != cleanup.len() {
        return Err(
            "Windows отклонила часть ввода и отпускание клавиш; отпустите Ctrl и V вручную".into(),
        );
    }
    Err(
        "Windows отклонила ввод. Возможно, целевое приложение запущено с правами администратора"
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn paste_does_not_leave_ctrl_down_between_batches() {
        let mut held = BTreeSet::new();
        let mut pastes = 0;
        let mut batches = 0;
        send_paste(|events| {
            batches += 1;
            for &(key, release) in events {
                if release {
                    held.remove(&key);
                } else {
                    held.insert(key);
                    if key == V && held.contains(&CTRL) {
                        pastes += 1;
                    }
                }
            }
            // Physical typing can resume after every SendInput batch.
            assert!(held.is_empty(), "typing here would become a Ctrl shortcut");
            events.len()
        })
        .unwrap();
        assert_eq!(pastes, 1);
        assert_eq!(batches, 1);
    }

    #[test]
    fn rejected_and_partial_batches_do_not_retry_paste_or_leave_keys_down() {
        for accepted in 0..4 {
            let mut batches = 0;
            let mut held = BTreeSet::new();
            let mut pastes = 0;
            let result = send_paste(|events| {
                batches += 1;
                let count = if batches == 1 { accepted } else { events.len() };
                if batches > 1 {
                    assert!(events.iter().all(|(_, release)| *release));
                }
                for &(key, release) in &events[..count] {
                    if release {
                        assert!(held.remove(&key), "released a key we did not press");
                    } else {
                        held.insert(key);
                        if key == V && held.contains(&CTRL) {
                            pastes += 1;
                        }
                    }
                }
                count
            });
            assert!(result.is_err());
            assert!(held.is_empty());
            assert_eq!(pastes, usize::from(accepted >= 2));
            assert_eq!(batches, if accepted == 0 { 1 } else { 2 });
        }
    }

    #[test]
    fn failed_release_is_reported_without_another_paste_attempt() {
        let mut calls = 0;
        let error = send_paste(|events| {
            calls += 1;
            if calls == 1 {
                2
            } else {
                assert!(events.iter().all(|(_, release)| *release));
                0
            }
        })
        .unwrap_err();
        assert_eq!(calls, 2);
        assert!(error.contains("отпустите Ctrl и V вручную"));
    }
}
