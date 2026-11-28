//! The egress boundary end to end: what the Privacy Shield branded is what goes on the wire, so the raw values never
//! do, and only the vault on the device turns the tokens back into them. That an un-redacted body does not compile is
//! shown by the `compile_fail` examples on `ServiceRequest` and on `somicus_privacy::Redacted`.

use somicus_net::{Method, NetError, ServiceRequest, send};
use somicus_privacy::PrivacyShield;
use somicus_uigraph::ClassifiedText;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;

/// Whether `received` holds a whole request: its head and the body its `Content-Length` announces.
fn is_complete(received: &[u8]) -> bool {
    let text = String::from_utf8_lossy(received);
    text.split_once("\r\n\r\n").is_some_and(|(head, body)| {
        head.lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .is_some_and(|length| body.len() >= length)
    })
}

#[tokio::test]
async fn only_the_shields_tokens_reach_the_wire() -> Result<(), NetError> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/v1/turn", listener.local_addr()?);
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await?;
        let mut received = Vec::new();
        let mut chunk = [0_u8; 1024];
        while !is_complete(&received) {
            let read = socket.read(&mut chunk).await?;
            if read == 0 {
                break;
            }
            received.extend_from_slice(&chunk[..read]);
        }
        socket
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
            .await?;
        Ok::<_, NetError>(String::from_utf8_lossy(&received).into_owned())
    });

    let mut shield = PrivacyShield::new();
    let body = shield
        .redact_text(&ClassifiedText::content_value(
            "Reply to jane.doe@example.com about IBAN DE89370400440532013000",
        ))
        .to_json_bytes()
        .map_err(|error| NetError::Connection(error.to_string()))?;
    let response = send(ServiceRequest {
        method: Method::Post,
        url: &url,
        body: Some(body),
        authorization: None,
        dpop: None,
    })
    .await?;
    assert_eq!(response.status, 204);

    let wire = server
        .await
        .map_err(|_| NetError::Connection("the test server stopped".to_owned()))??;
    assert!(wire.contains("⟦EMAIL_1⟧"), "{wire}");
    assert!(wire.contains("⟦IBAN_1⟧"), "{wire}");
    for raw in ["jane.doe@example.com", "DE89370400440532013000"] {
        assert!(!wire.contains(raw), "{raw} reached the wire");
    }
    assert!(shield.rehydrate(&wire).contains("jane.doe@example.com"));
    Ok(())
}
