use crate::discussion::Actor;
use crate::discussion::DiscussionMessage;
use crate::discussion::visible_messages;
use crate::tests::fixtures::screenshot;

fn message(body: &str, held: bool) -> DiscussionMessage {
	DiscussionMessage {
		author: Actor::User("anon-1".to_owned()),
		body: body.to_owned(),
		attachments: vec![screenshot("media-2")],
		held,
	}
}

#[test]
fn actor_roles_are_distinct() {
	let roles = [
		(Actor::System, false, true),
		(Actor::Ai, false, true),
		(Actor::User("u".to_owned()), false, false),
		(Actor::Maintainer("m".to_owned()), true, false),
	];
	for (actor, maintainer, automation) in roles {
		assert_eq!(actor.is_maintainer(), maintainer, "{actor:?}");
		assert_eq!(actor.is_automation(), automation, "{actor:?}");
	}
}

#[test]
fn held_messages_are_not_visible() {
	let thread = [
		message("first", false),
		message("held", true),
		message("last", false),
	];
	let bodies: Vec<_> = visible_messages(&thread)
		.map(|message| message.body.as_str())
		.collect();
	assert_eq!(bodies, ["first", "last"]);
}

#[test]
fn messages_round_trip_through_json() {
	let original = message("Here is a screenshot", false);
	let json = serde_json::to_value(&original).unwrap();
	assert_eq!(
		json["author"],
		serde_json::json!({"type": "user", "id": "anon-1"})
	);
	let decoded: DiscussionMessage = serde_json::from_value(json).unwrap();
	assert_eq!(decoded, original);
	let system: Actor =
		serde_json::from_value(serde_json::to_value(Actor::System).unwrap()).unwrap();
	assert_eq!(system, Actor::System);
}
