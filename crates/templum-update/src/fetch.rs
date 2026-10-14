//! GitHub over HTTPS: the newest release, and a file downloaded and checked against the SHA-256
//! GitHub lists for it.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use ureq::Agent;

use crate::{Asset, REPO, Release, VERSION, parse};

/// The HTTP client: Templum's name for GitHub (it rejects requests without one), the proxy from
/// `HTTPS_PROXY`.
pub fn agent() -> Agent {
    Agent::config_builder()
        .user_agent(format!("Templum/{VERSION}"))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        // ponytail: a limit for the whole download, not a stall detector; a download stalled
        // for good ends here and the next check starts it again.
        .timeout_global(Some(Duration::from_secs(30 * 60)))
        .build()
        .into()
}

/// The newest release that is neither a draft nor a prerelease.
pub fn latest(agent: &Agent) -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut answer = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(403 | 429) => {
                "GitHub's hourly limit for this network was reached".to_string()
            }
            ureq::Error::StatusCode(404) => "No release is published yet".to_string(),
            e => format!("Couldn't reach GitHub: {e}"),
        })?;
    let text = answer
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Couldn't read GitHub's answer: {e}"))?;
    parse(&text)
}

/// Downloads `asset` to `to` and keeps it only if its SHA-256 is the one GitHub lists; `progress`
/// hears the bytes received so far. A file already at `to` with that checksum is kept as it is.
pub fn download(
    agent: &Agent,
    asset: &Asset,
    to: &Path,
    progress: &mut dyn FnMut(u64),
) -> Result<(), String> {
    let Some(want) = asset.sha256.as_deref() else {
        return Err(format!("GitHub lists no checksum for {}", asset.name));
    };
    if sha256(to).is_ok_and(|have| have == want) {
        progress(asset.size);
        return Ok(());
    }
    let part = suffixed(to, ".part");
    let got = receive(agent, asset, &part, progress);
    let checked = match got {
        Ok(have) if have == want => fs::rename(&part, to).map_err(|e| cant_write(to, &e)),
        Ok(_) => Err(format!(
            "{} was damaged on the way (its checksum doesn't match)",
            asset.name
        )),
        Err(e) => Err(e),
    };
    if checked.is_err() {
        let _ = fs::remove_file(&part);
    }
    checked
}

/// Writes the body to `part` and returns its SHA-256.
fn receive(
    agent: &Agent,
    asset: &Asset,
    part: &Path,
    progress: &mut dyn FnMut(u64),
) -> Result<String, String> {
    let mut answer = agent
        .get(&asset.url)
        .call()
        .map_err(|e| format!("Couldn't download {}: {e}", asset.name))?;
    let mut body = answer.body_mut().as_reader();
    let mut file = File::create(part).map_err(|e| cant_write(part, &e))?;
    let mut hash = Sha256::new();
    let mut buf = vec![0; 64 * 1024];
    let mut got = 0;
    loop {
        let n = match body.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("Couldn't download {}: {e}", asset.name)),
        };
        file.write_all(&buf[..n])
            .map_err(|e| cant_write(part, &e))?;
        hash.update(&buf[..n]);
        got += n as u64;
        progress(got);
    }
    file.sync_all().map_err(|e| cant_write(part, &e))?;
    Ok(format!("{:x}", hash.finalize()))
}

/// A file's SHA-256, in lowercase hex.
fn sha256(path: &Path) -> io::Result<String> {
    let mut hash = Sha256::new();
    io::copy(&mut File::open(path)?, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

/// `path` with `suffix` added to its name: `x.deb` and `.part` make `x.deb.part`.
pub(crate) fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}

pub(crate) fn cant_write(path: &Path, e: &io::Error) -> String {
    format!("Couldn't write {}: {e}", path.display())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    use super::*;

    /// An HTTP server on localhost, for `connections` requests: `/moved` redirects to `/file`,
    /// which answers `body`. Returns its address.
    pub(crate) fn serve(body: &'static [u8], connections: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let here = address.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(connections) {
                let mut stream = stream.unwrap();
                let mut request = String::new();
                BufReader::new(&stream).read_line(&mut request).unwrap();
                let head = if request.starts_with("GET /moved ") {
                    format!("HTTP/1.1 302 Found\r\nLocation: {here}/file\r\nContent-Length: 0\r\n")
                } else {
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n", body.len())
                };
                let _ = stream.write_all(format!("{head}Connection: close\r\n\r\n").as_bytes());
                let _ = stream.write_all(body);
            }
        });
        address
    }

    pub(crate) fn asset(name: &str, url: String, body: &[u8]) -> Asset {
        Asset {
            name: name.to_string(),
            url,
            size: body.len() as u64,
            sha256: Some(format!("{:x}", Sha256::digest(body))),
        }
    }

    #[test]
    fn a_download_follows_the_redirect_and_is_checked() {
        const BODY: &[u8] = b"the new Templum";
        let address = serve(BODY, 2);
        let dir = tempfile::tempdir().unwrap();
        let to = dir.path().join("templum.deb");
        let mut heard = Vec::new();
        let asset = asset("templum.deb", format!("{address}/moved"), BODY);
        download(&agent(), &asset, &to, &mut |got| heard.push(got)).unwrap();
        assert_eq!(fs::read(&to).unwrap(), BODY);
        assert_eq!(heard.last(), Some(&(BODY.len() as u64)));
        assert!(!suffixed(&to, ".part").exists());

        // Already there and right: the server is gone, nothing is asked.
        heard.clear();
        download(&agent(), &asset, &to, &mut |got| heard.push(got)).unwrap();
        assert_eq!(heard, [BODY.len() as u64]);
    }

    #[test]
    fn a_damaged_download_leaves_nothing() {
        let address = serve(b"tampered", 1);
        let dir = tempfile::tempdir().unwrap();
        let to = dir.path().join("templum.AppImage");
        let asset = asset("templum.AppImage", format!("{address}/file"), b"original");
        let error = download(&agent(), &asset, &to, &mut |_| {}).unwrap_err();
        assert_eq!(
            error,
            "templum.AppImage was damaged on the way (its checksum doesn't match)"
        );
        assert!(!to.exists());
        assert!(!suffixed(&to, ".part").exists());
    }

    #[test]
    fn a_file_without_a_checksum_is_refused() {
        let asset = Asset {
            sha256: None,
            ..asset("x", "http://127.0.0.1:9/x".into(), b"")
        };
        let dir = tempfile::tempdir().unwrap();
        let error = download(&agent(), &asset, &dir.path().join("x"), &mut |_| {}).unwrap_err();
        assert_eq!(error, "GitHub lists no checksum for x");
    }
}
