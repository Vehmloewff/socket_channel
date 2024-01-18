use serde_json::{from_str, to_string, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SocketMessageError {
	#[error("Empty prefixes are not allowed")]
	EmptyPrefixNotAllowed,

	#[error("Context didn't finish because a closing parenthesis ')' was not found")]
	ExpectedClosingContextParen,
}

type Result<T> = error_stack::Result<T, SocketMessageError>;

#[derive(Debug)]
pub struct SocketMessage {
	text: String,
	prefix_end: usize,
	context_range: Option<(usize, usize)>,
	body_start: Option<usize>,
}

impl SocketMessage {
	pub fn parse<S: Into<String>>(text: S) -> Result<SocketMessage> {
		let text: String = text.into();
		let mut prefix_end = None;
		let mut context_start = None;
		let mut context_end = None;
		let mut body_start = None;

		let mut index = 0;

		for character in text.chars() {
			if prefix_end.is_none() {
				if character == '(' {
					prefix_end = Some(index);
				}
			} else if context_start.is_none() {
				context_start = Some(index);

				if character == ')' {
					context_end = Some(index);
				}
			} else if context_end.is_none() {
				if character == ')' {
					context_end = Some(index);
				}
			} else if body_start.is_none() {
				if !character.is_whitespace() {
					body_start = Some(index);
					break;
				}
			}

			index += 1;
		}

		let prefix_end = prefix_end.unwrap_or(text.len());

		if prefix_end == 0 {
			Err(SocketMessageError::EmptyPrefixNotAllowed)?
		}

		let context_range = match context_start {
			Some(context_start) => {
				let context_end = context_end.ok_or(SocketMessageError::ExpectedClosingContextParen)?;

				Some((context_start, context_end))
			}
			_ => None,
		};

		Ok(SocketMessage {
			text,
			prefix_end,
			context_range,
			body_start,
		})
	}

	pub fn get_prefix(&self) -> &str {
		&self.text[0..self.prefix_end]
	}

	pub fn get_context(&self) -> Option<&str> {
		match self.context_range {
			Some(range) => {
				if range.0 == range.1 {
					None
				} else {
					Some(&self.text[range.0..range.1])
				}
			}
			None => None,
		}
	}

	pub fn get_body(&self) -> Option<Value> {
		self.body_start.map(|index| from_str(&self.text[index..self.text.len()]).ok()).flatten()
	}

	pub fn get_str(&self) -> &str {
		&self.text
	}

	pub fn to_string(self) -> String {
		self.text
	}
}

pub struct SocketMessageBuilder<'a> {
	prefix: String,
	context: Option<String>,
	body: Option<&'a Value>,
}

impl<'a> SocketMessageBuilder<'a> {
	pub fn new<S: Into<String>>(prefix: S) -> SocketMessageBuilder<'a> {
		SocketMessageBuilder {
			prefix: prefix.into(),
			context: None,
			body: None,
		}
	}

	pub fn context<S: Into<String>>(mut self, context: S) -> SocketMessageBuilder<'a> {
		self.context = Some(context.into());

		self
	}

	pub fn body(mut self, body: &'a Value) -> SocketMessageBuilder<'a> {
		self.body = Some(body);

		self
	}

	pub fn build(self) -> Result<SocketMessage> {
		let mut text = self.prefix;
		let prefix_end = text.len();
		let mut context_range = None;
		let mut body_start = None;

		match self.context {
			Some(context) => {
				text.push_str("(");
				let context_start = text.len();

				text.push_str(&context);
				context_range = Some((context_start, text.len()));

				text.push_str(")");
			}
			None => (),
		};

		match self.body {
			Some(body) => {
				text.push_str(" ");
				body_start = Some(text.len());

				text.push_str(&to_string(&body).unwrap());
			}
			None => (),
		};

		Ok(SocketMessage {
			text,
			prefix_end,
			context_range,
			body_start,
		})
	}

	pub fn build_string(self) -> Result<String> {
		Ok(self.build()?.to_string())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pretty_assertions::assert_eq;
	use serde_json::json;

	#[test]
	fn parse_basic_message() {
		let message = SocketMessage::parse("some_prefix(context_here) { \"body\": \"here\" }").unwrap();

		assert_eq!(message.get_prefix(), "some_prefix");
		assert_eq!(message.get_context(), Some("context_here"));
		assert_eq!(message.get_body(), Some(json!({ "body": "here" })))
	}

	#[test]
	fn parse_message_with_missing_and_odd_order() {
		let message = SocketMessage::parse("prefix").unwrap();
		assert_eq!(message.get_prefix(), "prefix");
		assert_eq!(message.get_context(), None);
		assert_eq!(message.get_body(), None);

		let message = SocketMessage::parse("  prefix  ").unwrap();
		assert_eq!(message.get_prefix(), "  prefix  ");
		assert_eq!(message.get_context(), None);
		assert_eq!(message.get_body(), None);

		let message = SocketMessage::parse("prefix  (con text)   ").unwrap();
		assert_eq!(message.get_prefix(), "prefix  ");
		assert_eq!(message.get_context(), Some("con text"));
		assert_eq!(message.get_body(), None);

		let message = SocketMessage::parse("prefix  (con text)   [ \"item 1\" ,\"item2\"]  ").unwrap();
		assert_eq!(message.get_prefix(), "prefix  ");
		assert_eq!(message.get_context(), Some("con text"));
		assert_eq!(message.get_body(), Some(json!(["item 1", "item2"])));

		let message = SocketMessage::parse("prefix()[]").unwrap();
		assert_eq!(message.get_prefix(), "prefix");
		assert_eq!(message.get_context(), None);
		assert_eq!(message.get_body(), Some(json!([])));
	}

	#[test]
	fn parse_incorrect_message() {
		let _ = SocketMessage::parse("").unwrap_err();
		let _ = SocketMessage::parse("prefix( context_ no closing").unwrap_err();

		assert_eq!(SocketMessage::parse("hello () not_valid_json").unwrap().get_body(), None)
	}
}
