use super::*;

fn tabbed_snapshot() -> ClientShellSnapshot {
    let mut snapshot = snapshot();
    snapshot.workspaces.push(ClientShellWorkspace {
        workspace_id: "ws_2".into(),
        active_tab_id: "tab_2".into(),
        number: 2,
        label: "second".into(),
        focused: false,
        ..snapshot.workspaces[0].clone()
    });
    for number in 2..=3 {
        snapshot.tabs.push(ClientShellTab {
            tab_id: format!("tab_{number}"),
            workspace_id: "ws_2".into(),
            number: number - 1,
            label: if number == 3 { "review" } else { "shell" }.into(),
            focused: false,
            ..snapshot.tabs[0].clone()
        });
    }
    snapshot
}

fn tabbed_state() -> ClientShellState {
    let mut config = Config::default();
    config.ui.sidebar.spaces.show_tabs = true;
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(tabbed_snapshot()));
    state.set_pane_surface(surface());
    state
}

fn mouse(kind: MouseEventKind, rect: Rect) -> RawInputEvent {
    RawInputEvent::Mouse(MouseEvent {
        kind,
        column: rect.x + 1,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    })
}

#[test]
fn workspace_tab_children_are_opt_in_and_focus_without_arming_drag() {
    let mut state = tabbed_state();
    state.config.spaces.show_tabs = false;
    state.compose(106, 40).unwrap();
    assert!(state.hits.workspace_tabs.is_empty());

    state.config.spaces.show_tabs = true;
    state.compose(106, 40).unwrap();
    assert_eq!(state.hits.workspaces.len(), 2);
    assert_eq!(state.hits.workspace_tabs.len(), 3);
    let tab = state
        .hits
        .workspace_tabs
        .iter()
        .find(|hit| hit.tab_id == "tab_3")
        .unwrap();
    assert!(state
        .hits
        .workspaces
        .iter()
        .all(|hit| !super::super::contains(hit.rect, (tab.rect.x, tab.rect.y))));
    let outcome = state.handle_raw_events(vec![mouse(
        MouseEventKind::Down(MouseButton::Left),
        tab.rect,
    )]);
    assert!(
        matches!(outcome.actions.as_slice(), [ClientShellAction::Endpoint { request, .. }]
        if matches!(&request.method, crate::api::schema::Method::TabFocus(target) if target.tab_id == "tab_3"))
    );
    assert!(state.workspace_press.is_none());
    assert!(state.tab_press.is_none());
    assert!(state.chrome_drag.is_none());
}

#[test]
fn workspace_tab_children_scroll_independently_and_reveal_parent_on_focus_change() {
    let mut state = tabbed_state();
    let mut snapshot = snapshot();
    let tab = snapshot.tabs[0].clone();
    snapshot.tabs.extend((2..=12).map(|number| ClientShellTab {
        tab_id: format!("tab_{number}"),
        number,
        label: number.to_string(),
        focused: false,
        ..tab.clone()
    }));
    state.set_snapshot(Box::new(snapshot));
    state.compose(106, 20).unwrap();
    state.workspace_scroll = usize::MAX;
    state.compose(106, 20).unwrap();
    assert!(state.hits.workspaces.is_empty());
    assert_eq!(state.hits.workspace_tabs.last().unwrap().tab_id, "tab_12");

    state.reveal_focused_workspace = true;
    state.compose(106, 20).unwrap();
    assert_eq!(state.workspace_scroll, 0);
    assert_eq!(state.hits.workspaces[0].workspace_id, "ws_1");
}

#[test]
fn workspace_drag_ends_after_tab_children_and_excludes_clipped_children() {
    let mut state = tabbed_state();
    state.compose(106, 40).unwrap();
    let parent = state.hits.workspaces[0].rect;
    let last_tab = state.hits.workspace_tabs.last().unwrap().rect;
    let end = Rect::new(last_tab.x, last_tab.bottom(), last_tab.width, 1);
    state.handle_raw_events(vec![mouse(MouseEventKind::Down(MouseButton::Left), parent)]);
    state.handle_raw_events(vec![mouse(MouseEventKind::Drag(MouseButton::Left), end)]);
    assert!(
        matches!(state.chrome_drag, Some(ClientChromeDrag::Workspace {
        ref source_workspace_id, target: Some((None, row)),
    }) if source_workspace_id == "ws_1" && row == end.y)
    );

    state.handle_raw_events(vec![mouse(MouseEventKind::Up(MouseButton::Left), end)]);
    let mut snapshot = tabbed_snapshot();
    let tab = snapshot.tabs[2].clone();
    snapshot.tabs.extend((4..=15).map(|number| ClientShellTab {
        tab_id: format!("tab_{number}"),
        number,
        ..tab.clone()
    }));
    state.set_snapshot(Box::new(snapshot));
    state.compose(106, 24).unwrap();
    assert!(!state.hits.workspace_tabs.last().unwrap().last);
    let parent = state.hits.workspaces[0].rect;
    let bottom = Rect::new(
        parent.x,
        state.hits.workspace_body.bottom() - 1,
        parent.width,
        1,
    );
    state.handle_raw_events(vec![mouse(MouseEventKind::Down(MouseButton::Left), parent)]);
    state.handle_raw_events(vec![mouse(MouseEventKind::Drag(MouseButton::Left), bottom)]);
    assert!(matches!(
        state.chrome_drag,
        Some(ClientChromeDrag::Workspace {
            target: Some((Some(_), _)),
            ..
        })
    ));
}

#[test]
fn workspace_tab_children_keep_endpoint_identity_and_respect_collapsed_machines() {
    use crate::client::endpoint::{ProfileId, SavedSshEndpoint};
    let mut state = tabbed_state();
    let profile = SavedSshEndpoint {
        id: ProfileId::parse("0123456789abcdef0123456789abcdef").unwrap(),
        label: "Build".into(),
        target: "dev@build.example".into(),
        session: "agents".into(),
        enabled: true,
    };
    let remote = ClientEndpointId::Ssh(profile.id.clone());
    state.set_endpoint_catalog(&[profile]);
    state.set_endpoint_status(&remote, ClientEndpointStatus::Online);
    let mut snapshot = tabbed_snapshot();
    snapshot.boot_id = "remote-boot".into();
    state.set_endpoint_snapshot(&remote, Box::new(snapshot));
    state.compose(106, 60).unwrap();
    let tab = state
        .hits
        .workspace_tabs
        .iter()
        .find(|hit| hit.endpoint_id == remote && hit.tab_id == "tab_3")
        .unwrap();
    let outcome = state.handle_raw_events(vec![mouse(
        MouseEventKind::Down(MouseButton::Left),
        tab.rect,
    )]);
    assert!(
        matches!(outcome.actions.as_slice(), [ClientShellAction::ActivateEndpoint {
        endpoint_id, target: Some(ClientEndpointFocusTarget::Tab(tab_id)),
    }] if endpoint_id == &remote && tab_id == "tab_3")
    );
    assert!(state.workspace_press.is_none());
    assert!(state.tab_press.is_none());

    state.collapsed_endpoints.insert(remote.clone());
    state.compose(106, 60).unwrap();
    assert!(state
        .hits
        .workspace_tabs
        .iter()
        .all(|hit| hit.endpoint_id != remote));
}

#[test]
fn tab_titles_match_snapshot_revision_and_connection_generation() {
    use crate::protocol::endpoint::EndpointTabTitles;
    let mut state = tabbed_state();
    let local = ClientEndpointId::Local;
    let mut snapshot = tabbed_snapshot();
    let titles = EndpointTabTitles {
        boot_id: snapshot.boot_id.clone(),
        revision: 2,
        titles: std::collections::BTreeMap::from([("tab_3".into(), "session title".into())]),
    };
    state.set_endpoint_snapshot_for_generation(&local, 1, Box::new(snapshot.clone()));
    state.set_endpoint_tab_titles(&local, 1, titles.clone());
    assert_eq!(state.endpoints[0].tab_title("tab_3"), None);
    snapshot.revision = 2;
    state.set_endpoint_snapshot_for_generation(&local, 1, Box::new(snapshot.clone()));
    assert_eq!(state.endpoints[0].tab_title("tab_3"), Some("session title"));
    let mut pane_surface = surface();
    pane_surface.projection_revision = 2;
    state.set_pane_surface(pane_surface);
    state.sidebar_width = 36;
    let frame = state.compose(106, 40).unwrap();
    let row = state
        .hits
        .workspace_tabs
        .iter()
        .find(|hit| hit.tab_id == "tab_3")
        .unwrap()
        .rect
        .y;
    assert!(frame_rows(&frame)[usize::from(row)].contains("review · session title"));

    snapshot.revision = 3;
    state.set_endpoint_snapshot_for_generation(&local, 1, Box::new(snapshot.clone()));
    state.set_endpoint_tab_titles(&local, 1, titles.clone());
    assert_eq!(
        state.endpoints[0].tab_title("tab_3"),
        None,
        "older endpoints and stale data fall back to labels"
    );
    let mut pane_surface = surface();
    pane_surface.projection_revision = 3;
    state.set_pane_surface(pane_surface);
    let frame = state.compose(106, 40).unwrap();
    assert!(!frame_rows(&frame)[usize::from(row)].contains("session title"));

    let mut titles = titles;
    titles.revision = 3;
    state.set_endpoint_tab_titles(&local, 1, titles.clone());
    assert!(state.endpoints[0].tab_title("tab_3").is_some());
    state.set_endpoint_snapshot_for_generation(&local, 2, Box::new(snapshot.clone()));
    state.set_endpoint_tab_titles(&local, 1, titles.clone());
    assert_eq!(
        state.endpoints[0].tab_title("tab_3"),
        None,
        "old connection must not overwrite reconnected data"
    );
    titles.boot_id = "old-boot".into();
    state.set_endpoint_tab_titles(&local, 2, titles);
    assert_eq!(state.endpoints[0].tab_title("tab_3"), None);
}
