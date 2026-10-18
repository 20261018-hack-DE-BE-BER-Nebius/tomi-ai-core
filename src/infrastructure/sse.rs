use crate::api::error::ApiError;

#[derive(Default)]
pub struct SseDecoder {
    pending: Vec<u8>,
}
impl SseDecoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<String>, ApiError> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() > 1_048_576 {
            return Err(ApiError::bad("Provider event exceeded size limit"));
        }
        let mut records = Vec::new();
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = self.pending.drain(..=end).collect();
            let text = std::str::from_utf8(&line)
                .map_err(|_| ApiError::transport())?
                .trim();
            if let Some(data) = text.strip_prefix("data:") {
                records.push(data.trim().to_owned());
            }
        }
        Ok(records)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retains_split_utf8_and_frames() {
        let mut decoder = SseDecoder::default();
        let bytes = "data: {\"text\":\"é\"}\n\ndata: [DONE]\n".as_bytes();
        assert!(decoder.feed(&bytes[..17]).unwrap().is_empty());
        assert_eq!(
            decoder.feed(&bytes[17..]).unwrap(),
            vec!["{\"text\":\"é\"}", "[DONE]"]
        );
    }
}
