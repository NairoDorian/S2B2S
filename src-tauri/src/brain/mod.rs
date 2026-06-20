//! The "Brain": a streaming LLM subsystem for the Speech → Brain → Speech loop.
//!
//! Independent of dictation post-processing (`actions.rs`): the Brain streams an
//! OpenAI-compatible chat completion, emits tokens and completed sentences (the
//! latter feed streaming TTS), and supports mid-stream abort for barge-in.

pub mod client;
pub mod llama_manager;
pub mod manager;
