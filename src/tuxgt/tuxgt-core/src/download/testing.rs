pub(crate) fn serve_http(
    body: &'static [u8],
    advertised_len: usize,
    send_len: usize,
    n: usize,
) -> u16 {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for _ in 0..n {
            let Ok((mut s, _)) = listener.accept() else {
                break;
            };
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf);
            let hdr = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {advertised_len}\r\nConnection: close\r\n\r\n"
            );
            let _ = s.write_all(hdr.as_bytes());
            let _ = s.write_all(&body[..send_len.min(body.len())]);
        }
    });
    port
}
