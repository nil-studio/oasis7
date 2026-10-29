use super::*;

#[test]
fn archive_partial_frame_cannot_extend_the_attempt_deadline() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        for byte in [0u8, 0, 0, 1, 0] {
            if std::io::Write::write_all(&mut stream, &[byte]).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    let mut stream = TcpStream::connect(address).unwrap();
    let started = Instant::now();
    let mut limited = ArchiveDeadlineStream {
        stream: &mut stream,
        deadline: started + Duration::from_millis(35),
    };
    assert!(read_frame::<u32>(&mut limited).is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(stream);
    server.join().unwrap();
}
