# mp3info

**Experimental** CLI tool to read common metadata from MP3 files, with an ID3 parser written from scratch.

## Install
```sh
cargo install --root=$HOME/.local/ --path .
```

## Usage

- Display common tags:
```sh
mp3info info song.mp3
```

- View lyrics:
```sh
mp3info lyrics song.mp3
```

- Save cover photo:
```sh
mp3info picture song.mp3 > cover_front.jpg
```

Run `mp3info help` for detailed instructions.
