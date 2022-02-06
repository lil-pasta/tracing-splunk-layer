use std::collections::HashMap;
use std::time::{Instant, SystemTime};
use tracing::field::{Field, Value, Visit};
use tracing::span;
use tracing::Subscriber;
use tracing_subscriber::{
    layer::{Context, Layer},
    registry::LookupSpan,
};

// remove some boilerplate with this type alias for our events
// serde_json provides a convenient enum for valid json body values
pub type EventHash<'a> = HashMap<&'a str, serde_json::Value>;

// this is essentially a custom json layer implimentation
#[derive(Clone, Debug, serde::Serialize)]
pub struct EventStorage<'a>(EventHash<'a>);

impl<'a> EventStorage<'a> {
    pub fn new() -> Self {
        EventStorage::default()
    }

    pub fn events(&self) -> &EventHash {
        &self.0
    }
}

impl<'a> Default for EventStorage<'a> {
    fn default() -> Self {
        EventStorage(HashMap::new())
    }
}

// we need to impliment Visit to add the logic necessary to record a field of a specific
// type. (https://docs.rs/tracing-subscriber/0.3.6/tracing_subscriber/field/trait.Visit.html)
// we're basically just inserting field-value pairs into our EventStorage object
impl<'a> Visit for EventStorage<'a> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(
            field.name(),
            serde_json::Value::from(format!("{:?}", value)),
        );
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.insert(field.name(), serde_json::Value::from(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name(), serde_json::Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name(), serde_json::Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name(), serde_json::Value::from(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name(), serde_json::Value::from(value));
    }
}

// this is the actual layer which handles the tracing logic
pub struct SplunkHecLayer;

// TODO: track event and span metadata
// TODO: handle events not associated with a span
// TODO: ship a span to splunk once its been closed
impl<S> Layer<S> for SplunkHecLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    // on entering a new span we need to
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();

        // create a new visitor that inherits the parent's fields or gives us a fresh new visitor
        let mut event_visitor = if let Some(parent) = span.parent() {
            let mut extensions = parent.extensions_mut();
            extensions
                .get_mut::<EventStorage>()
                .map(|c| c.to_owned())
                .unwrap_or_default()
        } else {
            EventStorage::new()
        };

        // visit and record fields
        attrs.record(&mut event_visitor);

        // tracing_subscriber provides extensions on our spans so we can store span data
        // which the tracing library wont do.
        let mut extensions = span.extensions_mut();
        // store the fields
        extensions.insert::<EventStorage>(event_visitor);
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let span = ctx.lookup_current();
        if let Some(span) = &span {
            let mut extensions = span.extensions_mut();
            let event_visitor = extensions.get_mut::<EventStorage>().unwrap();
            event.record(event_visitor);
        } else {
            tracing::debug!("uh oh, this event doesn't have an associated span!")
        };
    }

    // allows us to update spans even after they are created
    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let mut extensions = span.extensions_mut();
        let event_visitor = extensions.get_mut::<EventStorage>().unwrap();
        values.record(event_visitor);
    }

    fn on_enter(&self, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let mut extensions = span.extensions_mut();

        // if you're entering a span for the first time then insert your the isntant otherwise dont
        // otherwise you won't find anything with the type Instant
        if extensions.get_mut::<Instant>().is_none() {
            extensions.insert(Instant::now());
        }
    }

    fn on_close(&self, id: span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(&id).unwrap();

        // u128 values aren't supported or something. luckily u64 is plenty precise for things like
        // web applications that operate at or above mili/micro second timescales.
        // this is also a convenient way to get the elapsed time and allow the extensions to drop
        // out of scope so we can get them later.
        let elapsed_time: u64 = {
            let extensions = span.extensions();
            extensions
                .get::<Instant>()
                .map(|t| t.elapsed().as_millis())
                // this should prevent us from failing
                .unwrap_or(0)
                .try_into()
                .unwrap()
        };

        let mut extensions = span.extensions_mut();
        let event_fields = extensions.get_mut::<EventStorage>().unwrap();
        event_fields
            .0
            .insert("elapsed_time", serde_json::to_value(elapsed_time).unwrap());
        println!("{}", serde_json::to_string_pretty(&event_fields).unwrap());
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
