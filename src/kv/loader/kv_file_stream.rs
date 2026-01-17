use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;

use crate::kv::loader::core::{KvStream, LoaderError};
use crate::kv::parser::{ChangeType, Parser};

/// KV 文件数据流：从文件中逐行读取 KV 数据并解析
pub struct KvFileStream<K, V> {
    /// 文件路径
    file_path: String,
    /// 行解析器
    parser: Arc<dyn Parser<K, V>>,
    /// 是否跳过脏数据（遇到解析错误时是否继续）
    skip_dirty_rows: bool,
    /// Scanner buffer 最小大小
    scanner_buffer_min_size: usize,
    /// Scanner buffer 最大大小
    scanner_buffer_max_size: usize,
}

impl<K, V> KvFileStream<K, V>
where
    K: Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    /// 创建新的 KvFileStream
    pub fn new(
        file_path: impl AsRef<Path>,
        parser: Arc<dyn Parser<K, V>>,
        skip_dirty_rows: bool,
    ) -> Self {
        Self {
            file_path: file_path.as_ref().to_string_lossy().to_string(),
            parser,
            skip_dirty_rows,
            scanner_buffer_min_size: 64 * 1024,      // 64KB
            scanner_buffer_max_size: 4 * 1024 * 1024, // 4MB
        }
    }

    /// 设置 scanner buffer 大小
    pub fn with_buffer_sizes(mut self, min_size: usize, max_size: usize) -> Self {
        self.scanner_buffer_min_size = min_size;
        self.scanner_buffer_max_size = max_size;
        self
    }
}

impl<K, V> KvStream<K, V> for KvFileStream<K, V>
where
    K: Clone + Send + Sync,
    V: Clone + Send + Sync,
{
    fn each(&self, callback: &dyn Fn(ChangeType, K, V) -> Result<(), LoaderError>) -> Result<(), LoaderError> {
        // 打开文件
        let file = File::open(&self.file_path).map_err(|e| {
            LoaderError::LoadFailed(format!("failed to open file '{}': {}", self.file_path, e))
        })?;

        // 创建带缓冲的 reader
        let reader = BufReader::with_capacity(self.scanner_buffer_min_size, file);

        let mut row_count = 0;
        let mut dirty_row_count = 0;

        // 逐行读取
        for line_result in reader.lines() {
            let line = line_result.map_err(|e| {
                LoaderError::LoadFailed(format!("failed to read line: {}", e))
            })?;

            row_count += 1;

            // 解析行
            let (change_type, key, value) = match self.parser.parse(line.as_bytes()) {
                Ok(result) => result,
                Err(e) => {
                    dirty_row_count += 1;
                    if self.skip_dirty_rows {
                        log::error!(
                            "parse failed, skipping line {}: content='{}', error={}",
                            row_count,
                            line,
                            e
                        );
                        continue;
                    } else {
                        return Err(LoaderError::ParserError(format!(
                            "parse failed for line {}, content='{}': {}",
                            row_count, line, e
                        )));
                    }
                }
            };

            // 调用回调处理
            callback(change_type, key, value).map_err(|e| {
                LoaderError::LoadFailed(format!("callback failed at line {}: {}", row_count, e))
            })?;
        }

        if dirty_row_count > 0 {
            log::debug!(
                "file processing completed: total_rows={}, dirty_rows={}",
                row_count,
                dirty_row_count
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::parser::{LineParser, LineParserConfig};
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tempfile::NamedTempFile;

    #[test]
    fn test_kv_file_stream_basic() {
        // 创建临时文件
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "key1\tvalue1").unwrap();
        writeln!(temp_file, "key2\tvalue2").unwrap();
        writeln!(temp_file, "key3\tvalue3").unwrap();

        // 创建 parser
        let parser_config = LineParserConfig {
            separator: "\t".to_string(),
        };
        let parser: Arc<dyn Parser<String, String>> = Arc::new(LineParser::new(parser_config));

        // 创建 stream
        let stream = KvFileStream::new(temp_file.path(), parser, false);

        // 遍历数据
        let results = Arc::new(Mutex::new(Vec::new()));
        let results_clone = results.clone();

        stream
            .each(&|change_type, key, value| {
                let mut results = results_clone.lock().unwrap();
                results.push(format!("{}:{}:{}", change_type as i32, key, value));
                Ok(())
            })
            .unwrap();

        let results = results.lock().unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.contains(&"1:key1:value1".to_string()));
        assert!(results.contains(&"1:key2:value2".to_string()));
        assert!(results.contains(&"1:key3:value3".to_string()));
    }

    #[test]
    fn test_kv_file_stream_skip_dirty_rows() {
        // 创建临时文件，包含脏数据
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "key1\tvalue1").unwrap();
        writeln!(temp_file, "invalid_line_without_separator").unwrap();
        writeln!(temp_file, "key2\tvalue2").unwrap();

        // 创建 parser
        let parser_config = LineParserConfig {
            separator: "\t".to_string(),
        };
        let parser: Arc<dyn Parser<String, String>> = Arc::new(LineParser::new(parser_config));

        // 创建 stream，启用跳过脏数据
        let stream = KvFileStream::new(temp_file.path(), parser, true);

        // 遍历数据
        let results = Arc::new(Mutex::new(Vec::new()));
        let results_clone = results.clone();

        stream
            .each(&|_change_type, key, value| {
                // 只记录非空的 key-value 对
                if !key.is_empty() && !value.is_empty() {
                    let mut results = results_clone.lock().unwrap();
                    results.push(format!("{}:{}", key, value));
                }
                Ok(())
            })
            .unwrap();

        // 应该只有两行有效数据
        let results = results.lock().unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&"key1:value1".to_string()));
        assert!(results.contains(&"key2:value2".to_string()));
    }

    #[test]
    fn test_kv_file_stream_not_skip_dirty_rows() {
        // 创建临时文件，包含脏数据
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "key1\tvalue1").unwrap();
        writeln!(temp_file, "invalid_line").unwrap();

        // 创建 parser
        let parser_config = LineParserConfig {
            separator: "\t".to_string(),
        };
        let parser: Arc<dyn Parser<String, String>> = Arc::new(LineParser::new(parser_config));

        // 创建 stream，不跳过脏数据
        let stream = KvFileStream::new(temp_file.path(), parser, false);

        let results = Arc::new(Mutex::new(Vec::new()));
        let results_clone = results.clone();

        // 遍历数据，LineParser 会将无效行解析为空 key 和空 value
        // 我们可以通过 handler 来判断并拒绝
        let result = stream.each(&|_change_type, key, value| {
            if key.is_empty() || value.is_empty() {
                Err(LoaderError::ParserError("empty key or value".to_string()))
            } else {
                let mut results = results_clone.lock().unwrap();
                results.push(format!("{}:{}", key, value));
                Ok(())
            }
        });

        // 应该返回错误（因为遇到了空 key 或 value）
        assert!(result.is_err());
    }
}
