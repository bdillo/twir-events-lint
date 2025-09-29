// use crate::linter::EventsByRegion;

// pub fn merge_events(draft_events: &[TwirEvent], new_events: &[TwirEvent]) -> Vec<TwirEvent> {
//     let mut events_map: HashMap<Vec<String>, TwirEvent> = HashMap::new();

//     for draft_event in draft_events {
//         events_map.insert(draft_event.event_key(), draft_event.clone());
//     }

//     for new_event in new_events {
//         let new_event_key = new_event.event_key();

//         if events_map.contains_key(&new_event_key) {
//             // if we have a match, it means we have the same event and need to take some action
//             let draft_event = events_map.get_mut(&new_event_key).unwrap();

//             if draft_event == new_event {
//                 // event hasn't changed, continue on
//                 debug!("keeping unchanged event {:?}", new_event_key);
//                 continue;
//             } else {
//                 // something has been updated - use the newer version of the event
//                 debug!("updated event {:?}", new_event_key);
//                 let _ = std::mem::replace(draft_event, new_event.clone());
//             }
//         } else {
//             debug!("found new event: {:?}", new_event_key);
//             events_map.insert(new_event_key, new_event.clone());
//         }
//     }
//     let mut updated_events: Vec<TwirEvent> = Vec::new();
//     for event in events_map.into_values() {
//         updated_events.push(event);
//     }

//     updated_events
// }
//

// pub fn read_new_events(events_json: &str) -> Result<EventsByRegion, serde_json::Error> {
//     serde_json::from_str(events_json)
// }
