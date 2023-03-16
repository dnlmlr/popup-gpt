pub struct SSEStream<T: std::io::Read> {
    source: T,
    buf: Vec<u8>,
    filled: usize,
}

impl<T: std::io::Read> SSEStream<T> {
    pub fn new(source: T) -> Self {
        Self {
            source,
            buf: vec![0; 1024 * 4],
            filled: 0,
        }
    }
}

impl<T: std::io::Read> Iterator for SSEStream<T> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buf.len() - self.filled < 128 {
            self.buf.resize_with(self.buf.len() * 2, || 0);
        }

        loop {
            let bytes_read = self.source.read(&mut self.buf[self.filled..]);

            match bytes_read {
                Ok(bytes_read) => {
                    self.filled += bytes_read;

                    let splitpos = String::from_utf8_lossy(&self.buf).find("\n\n");

                    if let Some(splitpos) = splitpos {
                        // skip 6 chars for "data: "
                        let data = &self.buf[6..splitpos];
                        let data = String::from_utf8_lossy(data).to_string();

                        if data == "[DONE]" {
                            return None;
                        }

                        // +2 because of "\n\n"
                        if self.filled > splitpos + 2 {
                            let filled = self.filled;
                            self.buf.copy_within(splitpos + 2..filled, 0);
                        }
                        self.filled -= splitpos + 2;

                        return Some(data);
                    }
                }
                Err(e) => {
                    eprintln!("{e}");
                    break;
                }
            }
        }

        None
    }
}
