use super::*;
use crate::protocol::endpoint::EndpointTabTitles;

#[derive(Clone, Debug)]
struct Projection {
    generation: u64,
    data: EndpointTabTitles,
}

impl Projection {
    fn matches(&self, generation: Option<u64>, snapshot: &ClientShellSnapshot) -> bool {
        Some(self.generation) == generation
            && self.data.boot_id == snapshot.boot_id
            && self.data.revision == snapshot.revision
    }
}

/// Keep optional titles separate from the frozen snapshot. A companion normally arrives
/// before its snapshot; neither reconnects nor newer snapshots may reuse an older title.
#[derive(Clone, Debug, Default)]
pub(super) struct TabTitles {
    current: Option<Projection>,
    pending: Option<Projection>,
}

impl TabTitles {
    fn receive(
        &mut self,
        generation: u64,
        data: EndpointTabTitles,
        snapshot_generation: Option<u64>,
        snapshot: Option<&ClientShellSnapshot>,
    ) {
        if snapshot_generation.is_some_and(|current| current > generation)
            || (snapshot_generation == Some(generation)
                && snapshot.is_some_and(|snapshot| {
                    snapshot.boot_id != data.boot_id || snapshot.revision > data.revision
                }))
        {
            return;
        }
        let next = Projection { generation, data };
        let slot = if snapshot.is_some_and(|snapshot| next.matches(snapshot_generation, snapshot)) {
            &mut self.current
        } else {
            &mut self.pending
        };
        if slot.as_ref().is_some_and(|previous| {
            previous.generation > generation
                || (previous.generation == generation
                    && previous.data.boot_id == next.data.boot_id
                    && previous.data.revision >= next.data.revision)
        }) {
            return;
        }
        *slot = Some(next);
    }

    pub(super) fn reconcile(
        &mut self,
        generation: Option<u64>,
        snapshot: Option<&ClientShellSnapshot>,
    ) {
        let Some(snapshot) = snapshot else {
            return;
        };
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.matches(generation, snapshot))
        {
            self.current = self.pending.take();
        }
        if self
            .current
            .as_ref()
            .is_some_and(|current| !current.matches(generation, snapshot))
        {
            self.current = None;
        }
        if self.pending.as_ref().is_some_and(|pending| {
            Some(pending.generation) != generation
                || pending.data.boot_id != snapshot.boot_id
                || pending.data.revision < snapshot.revision
        }) {
            self.pending = None;
        }
    }
}

impl ClientShellEndpoint {
    pub(super) fn tab_title(&self, tab_id: &str) -> Option<&str> {
        let snapshot = self.snapshot.as_deref()?;
        let current = self.tab_titles.current.as_ref()?;
        if !current.matches(self.snapshot_generation, snapshot) {
            return None;
        }
        current.data.titles.get(tab_id).map(String::as_str)
    }
}

impl ClientShellState {
    pub(crate) fn set_endpoint_tab_titles(
        &mut self,
        endpoint_id: &ClientEndpointId,
        generation: u64,
        titles: EndpointTabTitles,
    ) {
        if let Some(endpoint) = self
            .endpoints
            .iter_mut()
            .find(|endpoint| &endpoint.endpoint_id == endpoint_id)
        {
            endpoint.tab_titles.receive(
                generation,
                titles,
                endpoint.snapshot_generation,
                endpoint.snapshot.as_deref(),
            );
        }
    }
}
