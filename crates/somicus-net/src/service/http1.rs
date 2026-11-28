//! The HTTP/1.1 answer of the plain-HTTP path to a loopback address: the body after the head, with the chunked
//! transfer coding undone. A malformed answer gives what can be read, never a panic.

fn decode_chunked(mut data: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::new();
    while !data.is_empty() {
        let Some(newline_pos) = data.windows(2).position(|w| w == b"\r\n") else {
            break;
        };
        let line = String::from_utf8_lossy(&data[..newline_pos]);
        let hex_str = line.split(';').next().unwrap_or("").trim();
        let Ok(chunk_size) = usize::from_str_radix(hex_str, 16) else {
            break;
        };
        if chunk_size == 0 {
            break;
        }
        let chunk_start = newline_pos + 2;
        // A size from the network can be anything: past the end of the body is malformed, never an overflow.
        let Some(next_start) = chunk_start
            .checked_add(chunk_size)
            .filter(|end| *end <= data.len())
        else {
            break;
        };
        decoded.extend_from_slice(&data[chunk_start..next_start]);
        data = if data.len() >= next_start + 2 && &data[next_start..next_start + 2] == b"\r\n" {
            &data[next_start + 2..]
        } else {
            &data[next_start..]
        };
    }
    if decoded.is_empty() {
        data.to_vec()
    } else {
        decoded
    }
}

pub(crate) fn extract_response_body(raw_response: &[u8]) -> Vec<u8> {
    if let Some(sep_pos) = raw_response.windows(4).position(|w| w == b"\r\n\r\n") {
        let headers = String::from_utf8_lossy(&raw_response[..sep_pos]);
        let body = &raw_response[sep_pos + 4..];
        let is_chunked = headers.lines().any(|l| {
            l.to_ascii_lowercase().starts_with("transfer-encoding:")
                && l.to_ascii_lowercase().contains("chunked")
        });
        if is_chunked {
            decode_chunked(body)
        } else {
            body.to_vec()
        }
    } else {
        raw_response.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunk_size_beyond_the_body_is_not_a_panic() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\nabc\r\n0\r\n\r\n";
        assert!(extract_response_body(raw).len() <= raw.len());
    }
}
