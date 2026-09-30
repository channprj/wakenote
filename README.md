# WakeNote

[English README](README.en.md)

**macOS를 위한 로컬 우선 음성 캡처 및 전사 앱입니다.**

WakeNote는 마이크 한두 개, 지원되는 시스템 오디오 소스, 가져온 녹음 파일, 누르고 말하거나 토글 단축키로 시작·종료하는 받아쓰기를 다루는 메뉴 막대 녹음 앱입니다. 음성을 오래 보존할 수 있는 오디오·메타데이터·전사·보고서 파일로 만들며, 온디바이스 ASR과 명시적으로 설정한 클라우드 provider 중에서 선택할 수 있습니다.

데스크톱 앱은 캡처·영속성·전사를 위해 Tauri 2와 Rust를 사용하고, Capture·Meetings·Transcripts·Reports·Activity·Settings 작업 공간은 React 19와 TypeScript로 구성합니다.

## 주요 기능

- **자동 캡처** — 보정 가능한 dBFS gate와 attack/release timing, pre-roll, lead-in, post-roll로 긴 무음 구간을 계속 저장하지 않으면서 발화를 보존합니다.
- **유연한 듀얼 마이크** — Primary와 Secondary를 따로 저장하거나 적응형 동기화 후 합칠 수 있으며, Priority Audio로 두 입력을 섞지 않고 현재 가장 깨끗한 입력만 녹음할 수도 있습니다.
- **실시간 입력과 프롬프트 변환** — 오토타입 모델을 별도로 선택하고, 자막·전사·딕테이션의 번역 언어를 지정할 수 있습니다. Enhanced Prompt의 별도 단축키로 말한 초안을 프롬프트로 정리하며, 시스템 프롬프트도 수정할 수 있습니다. 번역과 프롬프트 변환은 선택적으로 켜는 OpenRouter 기능입니다.
- **로컬 및 opt-in 클라우드 ASR** — Whisper, sherpa-onnx, Qwen3-ASR는 로컬에서 실행되며 OpenRouter, OpenAI, Soniox는 해당 API key를 저장한 뒤에만 사용할 수 있습니다.
- **받아쓰기와 기록 확인** — Hold-to-dictate hotkey와 Toggle dictation hotkey를 따로 지정할 수 있습니다. Recent Dictations에서 최근 결과를 읽고 복사하거나 해당 날짜로 이동하며, Transcripts의 Dictations 필터로 받아쓰기만 모아 볼 수 있습니다.
- **실시간 및 보관 workflow** — 커스터마이징 가능한 Subtitle, 페이지 처리된 날짜별 Transcripts·Activity 기록, M4A 회의 녹음, Markdown 보고서가 동일한 model 및 Dictionary 계약을 공유합니다.
- **지속 가능한 복구** — queue job은 재시작 후에도 유지되고, running 작업은 안전하게 취소할 수 있으며, 선택한 녹음 bundle은 텍스트·JSON·오디오를 빠뜨리지 않고 macOS 휴지통으로 함께 이동합니다.
- **확인 가능한 저장 구조** — 오디오, 메타데이터, 전사, 복구 가능한 오류, 회의, 보고서가 사용자가 선택한 save root 아래에 남습니다.

## 설치

현재 WakeNote 문서는 소스 빌드를 기준으로 합니다. Node.js 20+, `pnpm` 10, stable Rust toolchain, Xcode Command Line Tools, CMake를 설치하세요. macOS에서는 `brew install cmake`로 CMake를 설치한 뒤 다음을 실행하세요.

```bash
pnpm install
pnpm tauri dev
```

macOS 권한, 선택 모델별 추가 요구사항, release build, 로컬 설치, DMG packaging은 [사용법](USAGE.md#installation)을 참고하세요.

## 로컬 릴리스

이 저장소의 GitHub Actions는 비활성화되어 있습니다. Mac에서 빌드한 뒤 명령을 직접 실행해 게시합니다.

```bash
pnpm release:build
pnpm release:publish --dry-run
pnpm release:publish
```

빌드 전에 버전을 동기화하고 커밋하세요. 게시 전에 브랜치와 annotated 버전 태그도 푸시해야 합니다. 산출물은 `release/v<version>/<architecture>/`에 저장되며, 현재 Mac의 아키텍처용으로 빌드됩니다. 앱은 ad-hoc 서명을 사용하고 공증되지 않습니다. 필요한 도구, 초안 릴리스와 실패 시 처리 방법은 [릴리스 절차](USAGE.md#local-build-and-manual-github-release)를 참고하세요.

## 빠른 시작

1. **Settings › Audio**에서 Microphone 접근을 허용하고 Primary 입력을 선택하세요. 필요할 때만 서로 다른 Secondary 입력을 추가하세요.
2. **Settings › Storage**에서 save root를 선택하고 확인하세요.
3. **Settings › Models**에서 model을 설치하거나 선택하세요. 클라우드 key는 **Settings › Integrations**에서만 추가하세요.
4. **Capture**로 돌아가 말할 때 live level이 설정한 threshold를 넘는지 확인하세요.
5. 완료된 결과는 **Transcripts**, queue 결과는 **Activity**에서 검토하세요.

기본 archive root는 `~/Documents/WakeNote`이고 기본 local model은 `whisper-medium`입니다.

## 자세한 문서

- [사용법](USAGE.md) — 설치, 명령, 설정, model capability, 예제, 문제 해결
- [아키텍처](ARCHITECTURE.md) — component, data flow, 영속성 경계, 설계 결정

## 라이선스

WakeNote는 [MIT 라이선스](LICENSE)로 배포됩니다. 포함된 외부 구성 요소에는 각각의 라이선스가 적용됩니다.
