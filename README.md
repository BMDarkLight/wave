# Wave

**Wave** is a lightweight, cross-platform, and portable music player built with modern technologies.  
It focuses on **performance**, **simplicity**, and **offline-first usage**, while remaining easily extensible for advanced audio features.

---

## Features

- Local music library (folder-based)
- High-performance audio playback
- Cross-platform (Windows, macOS, Linux)
- Portable & lightweight (no heavy runtime)
- Designed for future EQ & DSP extensions
- Offline-first (no server required)

---

## Tech Stack

### Frontend
- **React**
- **TypeScript**
- **Vite**

### App Shell
- **Tauri** (lightweight native shell)

### Backend / Audio Engine
- **Rust**
- **Rodio** – audio playback
- **Symphonia** – audio decoding
- **CPAL** – low-level audio backend

### Storage
- **SQLite** – music library, playlists, settings

---

## Project Structure

```
Wave/
├── src/              # React frontend
│   ├── utils/
│   ├── App.tsx
│   ├── App.css
│   └── main.tsx
│
├── src-tauri/        # Rust backend
│
├── README.md
└── tauri.conf.json
```

---

## Getting Started

### Prerequisites

Make sure you have the following installed:

- **Node.js** (LTS)
- **Rust**
- **Cargo**
- **Git**

Verify installation:

```bash
node -v
rustc --version
cargo --version
```

---

### Install Dependencies

```bash
npm install
```

---

### Run in Development Mode

```bash
npm run tauri dev
```

This will start both the frontend and the native backend.

---

## Build for Production

```bash
npm run tauri build
```

The final portable binaries will be generated for your platform.

---

## Design Goals

- **Fast startup**
- **Low memory usage**
- **Clean architecture**
- **No unnecessary dependencies**
- **Long-term maintainability**

---

## License

This project is licensed under the **MIT License**.

---

## Author

Built with ❤️ by **Behdad**
