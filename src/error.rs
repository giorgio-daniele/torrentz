#[derive(Debug)]
pub enum ApplicationError {
    ParserError(String),
    TrackerError(String),
    ProtocolError(String),
    PeerError(String),
    WorkerError(String),
}
