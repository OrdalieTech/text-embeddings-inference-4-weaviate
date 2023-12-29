use crate::ErrorType;
use serde::de::{SeqAccess, Visitor};
use serde::{de, Deserialize, Deserializer, Serialize};
use serde_json::json;
use std::fmt::Formatter;
use text_embeddings_core::tokenization::EncodingInput;
use utoipa::openapi::{RefOr, Schema};
use utoipa::ToSchema;

#[derive(Debug)]
pub(crate) enum Sequence {
    Single(String),
    Pair(String, String),
}

impl Sequence {
    pub(crate) fn count_chars(&self) -> usize {
        match self {
            Sequence::Single(s) => s.chars().count(),
            Sequence::Pair(s1, s2) => s1.chars().count() + s2.chars().count(),
        }
    }
}

impl From<Sequence> for EncodingInput {
    fn from(value: Sequence) -> Self {
        match value {
            Sequence::Single(s) => Self::Single(s),
            Sequence::Pair(s1, s2) => Self::Dual(s1, s2),
        }
    }
}

#[derive(Debug)]
pub(crate) enum PredictInput {
    Single(Sequence),
    Batch(Vec<Sequence>),
}

impl<'de> Deserialize<'de> for PredictInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Internal {
            Single(String),
            Multiple(Vec<String>),
        }

        struct PredictInputVisitor;

        impl<'de> Visitor<'de> for PredictInputVisitor {
            type Value = PredictInput;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "a string, \
                    a pair of strings [string, string] \
                    or a batch of mixed strings and pairs [[string], [string, string], ...]",
                )
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(PredictInput::Single(Sequence::Single(v.to_string())))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let sequence_from_vec = |mut value: Vec<String>| {
                    // Validate that value is correct
                    match value.len() {
                        1 => Ok(Sequence::Single(value.pop().unwrap())),
                        2 => {
                            // Second element is last
                            let second = value.pop().unwrap();
                            let first = value.pop().unwrap();
                            Ok(Sequence::Pair(first, second))
                        }
                        // Sequence can only be a single string or a pair of strings
                        _ => Err(de::Error::invalid_length(value.len(), &self)),
                    }
                };

                // Get first element
                // This will determine if input is a batch or not
                let s = match seq
                    .next_element::<Internal>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?
                {
                    // Input is not a batch
                    // Return early
                    Internal::Single(value) => {
                        // Option get second element
                        let second = seq.next_element()?;

                        if seq.next_element::<String>()?.is_some() {
                            // Error as we do not accept > 2 elements
                            return Err(de::Error::invalid_length(3, &self));
                        }

                        if let Some(second) = second {
                            // Second element exists
                            // This is a pair
                            return Ok(PredictInput::Single(Sequence::Pair(value, second)));
                        } else {
                            // Second element does not exist
                            return Ok(PredictInput::Single(Sequence::Single(value)));
                        }
                    }
                    // Input is a batch
                    Internal::Multiple(value) => sequence_from_vec(value),
                }?;

                let mut batch = Vec::with_capacity(32);
                // Push first sequence
                batch.push(s);

                // Iterate on all sequences
                while let Some(value) = seq.next_element::<Vec<String>>()? {
                    // Validate sequence
                    let s = sequence_from_vec(value)?;
                    // Push to batch
                    batch.push(s);
                }
                Ok(PredictInput::Batch(batch))
            }
        }

        deserializer.deserialize_any(PredictInputVisitor)
    }
}

impl<'__s> ToSchema<'__s> for PredictInput {
    fn schema() -> (&'__s str, RefOr<Schema>) {
        (
            "PredictInput",
            utoipa::openapi::OneOfBuilder::new()
                .item(
                    utoipa::openapi::ObjectBuilder::new()
                        .schema_type(utoipa::openapi::SchemaType::String)
                        .description(Some("A single string")),
                )
                .item(
                    utoipa::openapi::ArrayBuilder::new()
                        .items(
                            utoipa::openapi::ObjectBuilder::new()
                                .schema_type(utoipa::openapi::SchemaType::String),
                        )
                        .description(Some("A pair of strings"))
                        .min_items(Some(2))
                        .max_items(Some(2)),
                )
                .item(
                    utoipa::openapi::ArrayBuilder::new().items(
                        utoipa::openapi::OneOfBuilder::new()
                            .item(
                                utoipa::openapi::ArrayBuilder::new()
                                    .items(
                                        utoipa::openapi::ObjectBuilder::new()
                                            .schema_type(utoipa::openapi::SchemaType::String),
                                    )
                                    .description(Some("A single string"))
                                    .min_items(Some(1))
                                    .max_items(Some(1)),
                            )
                            .item(
                                utoipa::openapi::ArrayBuilder::new()
                                    .items(
                                        utoipa::openapi::ObjectBuilder::new()
                                            .schema_type(utoipa::openapi::SchemaType::String),
                                    )
                                    .description(Some("A pair of strings"))
                                    .min_items(Some(2))
                                    .max_items(Some(2)),
                            )
                    ).description(Some("A batch")),
                )
                .description(Some(
                    "Model input. \
                Can be either a single string, a pair of strings or a batch of mixed single and pairs \
                of strings.",
                ))
                .example(Some(json!("What is Deep Learning?")))
                .into(),
        )
    }
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct PredictRequest {
    pub inputs: PredictInput,
    #[serde(default)]
    #[schema(default = "false", example = "false")]
    pub truncate: bool,
    #[serde(default)]
    #[schema(default = "false", example = "false")]
    pub raw_scores: bool,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct Prediction {
    #[schema(example = "0.5")]
    pub score: f32,
    #[schema(example = "admiration")]
    pub label: String,
}

#[derive(Serialize, ToSchema)]
#[serde(untagged)]
pub(crate) enum PredictResponse {
    Single(Vec<Prediction>),
    Batch(Vec<Vec<Prediction>>),
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct RerankRequest {
    #[schema(example = "What is Deep Learning?")]
    pub query: String,
    #[schema(example = json!(["Deep Learning is ..."]))]
    pub texts: Vec<String>,
    #[serde(default)]
    #[schema(default = "false", example = "false")]
    pub truncate: bool,
    #[serde(default)]
    #[schema(default = "false", example = "false")]
    pub raw_scores: bool,
    #[serde(default)]
    #[schema(default = "false", example = "false")]
    pub return_text: bool,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct Rank {
    #[schema(example = "0")]
    pub index: usize,
    #[schema(nullable = true, example = "Deep Learning is ...", default = "null")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[schema(example = "1.0")]
    pub score: f32,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct RerankResponse(pub Vec<Rank>);

#[derive(Deserialize, ToSchema)]
#[serde(untagged)]
pub(crate) enum Input {
    Single(String),
    Batch(Vec<String>),
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct OpenAICompatRequest {
    pub input: Input,
    #[allow(dead_code)]
    #[schema(nullable = true, example = "null")]
    pub model: Option<String>,
    #[allow(dead_code)]
    #[schema(nullable = true, example = "null")]
    pub user: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct OpenAICompatEmbedding {
    #[schema(example = "embedding")]
    pub object: &'static str,
    #[schema(example = json!([0.0, 1.0, 2.0]))]
    pub embedding: Vec<f32>,
    #[schema(example = "0")]
    pub index: usize,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct OpenAICompatUsage {
    #[schema(example = "512")]
    pub prompt_tokens: usize,
    #[schema(example = "512")]
    pub total_tokens: usize,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct OpenAICompatResponse {
    #[schema(example = "list")]
    pub object: &'static str,
    pub data: Vec<OpenAICompatEmbedding>,
    #[schema(example = "thenlper/gte-base")]
    pub model: String,
    pub usage: OpenAICompatUsage,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct EmbedRequest {
    pub inputs: Input,
    #[serde(default)]
    #[schema(default = "false", example = "false")]
    pub truncate: bool,
    #[serde(default = "default_normalize")]
    #[schema(default = "true", example = "true")]
    pub normalize: bool,
}

fn default_normalize() -> bool {
    true
}

#[derive(Serialize, ToSchema)]
#[schema(example = json!([[0.0, 1.0, 2.0]]))]
pub(crate) struct EmbedResponse(pub Vec<Vec<f32>>);

#[derive(Deserialize, ToSchema, Debug)]
pub(crate) struct EmbedWeaviateRequest {
    pub text: String,
    #[serde(default)]
    #[schema(default = "false", example = "false")]
    pub truncate: bool,
    #[serde(default = "default_normalize")]
    #[schema(default = "true", example = "true")]
    pub normalize: bool,
}

#[derive(Serialize, ToSchema, Debug)]
pub(crate) struct EmbedWeaviateResponse {
    pub text: String,
    pub vector: Vec<f32>,
    pub dim: usize,
}


#[derive(Serialize, ToSchema)]
pub(crate) struct OpenAICompatErrorResponse {
    pub message: String,
    pub code: u16,
    #[serde(rename(serialize = "type"))]
    pub error_type: ErrorType,
}
