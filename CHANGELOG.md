# Changelog

## 0.1.1

### Fixes

- Zoom editor, note and Git text smoothly, keeping the lines you were reading
- Wrap a note's text at the same words at every zoom
- Keep a zooming editor's text under the pointer, and its padding even
- Never miss a terminal's last output, nor keep a stale command dot

### Performance

- Keep a Git block fluid with hundreds of changed files
- Stop editor and note text flickering and stalling while zooming
- Rebuild only the rows the terminal reports as changed
- Read a terminal's grid only after it changed
- Keep off-screen output from redrawing, and never wait for a whole PTY read
- Paint the drawings without copying them every frame
- Draw the dot grid as one layer and never more than 3,700 dots

## 0.1.0

First release.

