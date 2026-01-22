pub mod bson_serializer;
pub mod core;
pub mod json_serializer;
pub mod msgpack_serializer;
pub mod protobuf_serializer;
pub mod register;

// 重新导出核心类型和 trait
pub use core::{Serializer, SerializerError};

// 重新导出具体的序列化器
pub use bson_serializer::{BsonSerializer, BsonSerializerConfig};
pub use json_serializer::{JsonSerializer, JsonSerializerConfig};
pub use msgpack_serializer::{MsgPackSerializer, MsgPackSerializerConfig};
pub use protobuf_serializer::{ProtobufSerializer, ProtobufSerializerConfig};

// 重新导出注册函数
pub use register::{
    register_protobuf_serializers, register_serde_serializers, register_serializers,
};
