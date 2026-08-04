# WakeNote

[English README](README.md)

**macOS를 위한 로컬 우선 음성 캡처 및 전사 앱입니다.**

WakeNote는 마이크 한두 개, 지원되는 시스템 오디오 소스, 가져온 녹음 파일, push-to-talk 받아쓰기를 다루는 메뉴 막대 녹음 앱입니다. 음성을 오래 보존할 수 있는 오디오·메타데이터·전사·보고서 파일로 만들며, 온디바이스 ASR과 명시적으로 설정한 클라우드 provider 중에서 선택할 수 있습니다.

데스크톱 앱은 캡처·영속성·전사를 위해 Tauri 2와 Rust를 사용하고, Capture·Meetings·Transcripts·Reports·Activity·Settings 작업 공간은 React 19와 TypeScript로 구성합니다.

## 주요 기능

- **자동 캡처** — 보정 가능한 dBFS gate와 attack/release timing, pre-roll, lead-in, post-roll로 긴 무음 구간을 계속 저장하지 않으면서 발화를 보존합니다.
- **안정적인 듀얼 마이크** — Primary와 선택적 Secondary 입력이 독립된 stream과 복구 상태를 유지하고, 기본적으로 하나의 시간 정렬된 녹음으로 합쳐집니다.
- **로컬 및 opt-in 클라우드 ASR** — Whisper, sherpa-onnx, Qwen3-ASR는 로컬에서 실행되며 OpenRouter, OpenAI, Soniox는 해당 API key를 저장한 뒤에만 사용할 수 있습니다.
- **실시간 및 보관 workflow** — floating caption, 단축키 받아쓰기, 날짜별 전사 기록, 장시간 회의, Markdown 보고서가 동일한 model 및 Dictionary 계약을 공유합니다.
- **지속 가능한 복구** — queue job은 재시작 후에도 유지되고, 중단된 running job은 pending으로 돌아가며, warning은 녹음을 삭제하지 않고 계속 검토할 수 있습니다.
- **확인 가능한 저장 구조** — 오디오, 메타데이터, 전사, 복구 가능한 오류, 회의, 보고서가 사용자가 선택한 save root 아래에 남습니다.

## 설치

현재 WakeNote 문서는 소스 빌드를 기준으로 합니다. Node.js 20+, `pnpm` 10, stable Rust toolchain, Xcode Command Line Tools를 설치한 뒤 다음을 실행하세요.

```bash
pnpm install
pnpm tauri dev
```

macOS 권한, 선택 모델별 추가 요구사항, release build, 로컬 설치, DMG packaging은 [사용법](USAGE.md#installation)을 참고하세요.

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
- [제품 요구사항](PRD.md) — 한국어 제품 specification과 acceptance criteria

## 라이선스

아직 라이선스가 명시되지 않았습니다.
