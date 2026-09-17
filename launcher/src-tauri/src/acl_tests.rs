use tauri::ipc::{Origin, RuntimeAuthority};
use tauri_utils::{acl::resolved::Resolved, platform::Target};

// Exercise Tauri's actual ACL resolver against build-generated manifests,
// not the browser mock of invoke(). No GUI or running server is needed.
#[test]
fn loopback_webview_can_use_app_commands_but_other_origins_cannot() {
    let manifests = serde_json::from_str(include_str!(concat!(
        env!("OUT_DIR"),
        "/acl-manifests.json"
    )))
    .unwrap();
    let capabilities =
        serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/capabilities.json"))).unwrap();
    let resolved = Resolved::resolve(&manifests, capabilities, Target::Windows).unwrap();
    let authority = RuntimeAuthority::new(manifests, resolved);
    for command in [
        "save_history_export",
        "templates_desktop_state",
        "ack_templates_activation",
        "set_template_hotkey",
        "record_template_key",
        "cancel_template_recording",
        "choose_templates_file",
        "copy_template",
        "paste_template",
    ] {
        for url in ["http://127.0.0.1:11434/", "http://127.0.0.1:23456/"] {
            let origin = Origin::Remote {
                url: url.parse().unwrap(),
            };
            assert!(
                authority
                    .resolve_access(command, "main", "main", &origin)
                    .is_some(),
                "{command} denied for {url}"
            );
            assert!(authority
                .resolve_access(command, "other", "other", &origin)
                .is_none());
        }
        for url in [
            "https://example.com/",
            "http://localhost:11434/",
            "http://127.0.0.2:11434/",
        ] {
            let origin = Origin::Remote {
                url: url.parse().unwrap(),
            };
            assert!(
                authority
                    .resolve_access(command, "main", "main", &origin)
                    .is_none(),
                "{command} exposed to {url}"
            );
        }
    }
    let origin = Origin::Remote {
        url: "http://127.0.0.1:11434/".parse().unwrap(),
    };
    assert!(authority
        .resolve_access("unregistered_command", "main", "main", &origin)
        .is_none());
    assert!(authority
        .resolve_access(
            "plugin:clipboard-manager|read_text",
            "main",
            "main",
            &origin
        )
        .is_none());
}
