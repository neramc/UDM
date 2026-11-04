#!/usr/bin/env python3
"""Regenerates crates/udm-gui/lang/ko/LC_MESSAGES/udm-gui.po.

Extracts every @tr("...") string from crates/udm-gui/ui/*.slint (no
translation context; build.rs uses DefaultTranslationContext::None) and
merges the Korean translations kept in KO below. Missing translations are
written with an empty msgstr (falls back to English at runtime) and listed on
stderr, so untranslated strings are never hidden.

Usage: python3 -I tools/update_po.py
"""
import glob
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
UI = os.path.join(ROOT, "crates", "udm-gui", "ui")
OUT = os.path.join(ROOT, "crates", "udm-gui", "lang", "ko", "LC_MESSAGES", "udm-gui.po")

KO = {
    "Simulated": "시뮬레이션",
    "System disk": "시스템 디스크",
    "System use unknown": "시스템 사용 여부 알 수 없음",
    "No media": "미디어 없음",
    "Write-protected": "쓰기 보호됨",
    "In use": "사용 중",
    "Unidentified device": "식별되지 않은 장치",
    "Size unknown": "크기 알 수 없음",
    "USB": "USB",
    "NVMe": "NVMe",
    "ATA/SATA": "ATA/SATA",
    "SCSI/SAS": "SCSI/SAS",
    "SD/MMC": "SD/MMC",
    "Virtual (virtio)": "가상 (virtio)",
    "Unknown transport": "연결 방식 알 수 없음",
    "Raw read": "원시 읽기",
    "Raw write": "원시 쓰기",
    "Safe eject": "안전하게 꺼내기",
    "SMART / health data": "SMART / 상태 정보",
    "Partition editing": "파티션 편집",
    "Secure erase": "보안 삭제",
    "Filesystem check": "파일 시스템 검사",
    "Supported": "지원됨",
    "Limited": "제한됨",
    "Unavailable": "사용 불가",
    "Not tested": "테스트되지 않음",
    "Nobody has assessed this capability for this device.": "이 장치에 대해 이 기능을 평가한 적이 없습니다.",
    "Not implemented in this build. Phase 0 is read-only.": "이 빌드에는 구현되어 있지 않습니다. Phase 0은 읽기 전용입니다.",
    "Requires the privileged helper.": "권한 도우미 프로세스가 필요합니다.",
    "Permission denied for the device node.": "장치 노드에 대한 권한이 없습니다.",
    "The device reports write protection.": "장치가 쓰기 보호 상태를 보고합니다.",
    "No media is present.": "미디어가 없습니다.",
    "Not available over this transport.": "이 연결 방식에서는 사용할 수 없습니다.",
    "USB bridge pass-through has not been verified.": "USB 브리지의 명령 전달 지원 여부가 확인되지 않았습니다.",
    "Overwriting flash memory cannot guarantee physical erasure.": "플래시 메모리는 덮어쓰기로 물리적 삭제를 보장할 수 없습니다.",
    "Vendor": "제조사",
    "Model": "모델",
    "Revision": "리비전",
    "Serial number": "일련번호",
    "World Wide Name": "WWN",
    "USB vendor ID": "USB 제조사 ID",
    "USB product ID": "USB 제품 ID",
    "USB manufacturer": "USB 제조사",
    "USB product": "USB 제품",
    "USB serial number": "USB 일련번호",
    "Reported capacity": "보고된 용량",
    "Logical block size": "논리 블록 크기",
    "Physical block size": "물리 블록 크기",
    "Persistent link": "영구 링크",
    "OS device name": "OS 장치 이름",
    "OS device number": "OS 장치 번호",
    "High confidence": "신뢰도 높음",
    "Medium confidence": "신뢰도 보통",
    "Low confidence": "신뢰도 낮음",
    "Simulator": "시뮬레이터",
    "Linux sysfs": "Linux sysfs",
    "udev link": "udev 링크",
    "Mounted": "마운트됨",
    "Active swap": "활성 스왑",
    "Held by": "점유 장치",
    "Some information could not be read.": "일부 정보를 읽을 수 없었습니다.",
    "Several devices report the same serial number; it is not used to identify them.": "여러 장치가 같은 일련번호를 보고합니다. 이 번호는 식별에 사용되지 않습니다.",
    "No usable serial number or WWN: this device's identity cannot be confirmed.": "사용할 수 있는 일련번호나 WWN이 없어 이 장치의 신원을 확인할 수 없습니다.",
    "The disk holding the running system could not be determined. All devices are treated as possibly in use by the system.": "실행 중인 시스템이 있는 디스크를 확인할 수 없습니다. 모든 장치를 시스템이 사용 중일 수 있는 것으로 취급합니다.",
    "Virtual block devices (loop, device-mapper, RAID) are not listed.": "가상 블록 장치(loop, device-mapper, RAID)는 표시하지 않습니다.",
    "Device discovery is not implemented for this operating system in this build.": "이 빌드에는 이 운영체제용 장치 검색이 구현되어 있지 않습니다.",
    "Permission denied while listing storage devices.": "저장 장치 목록을 읽는 중 권한이 거부되었습니다.",
    "The list of storage devices could not be read.": "저장 장치 목록을 읽을 수 없습니다.",
    "Simulated discovery failure.": "시뮬레이션된 검색 실패입니다.",
    "Device discovery stopped unexpectedly.": "장치 검색이 예기치 않게 중단되었습니다.",
    "The background discovery worker is not running.": "백그라운드 검색 작업자가 실행 중이 아닙니다.",
    "Present": "있음",
    "Unknown": "알 수 없음",
    "Not write-protected": "쓰기 보호되지 않음",
    "Holds the running system": "실행 중인 시스템 포함",
    "Not a system disk": "시스템 디스크 아님",
    "Unknown (treated as system disk)": "알 수 없음 (시스템 디스크로 취급)",
    "Yes": "예",
    "No (as reported; many USB drives report this)": "아니요 (보고된 값이며, 많은 USB 드라이브가 이렇게 보고합니다)",
    "Not reported": "보고되지 않음",
    "Universal Drive Manager": "Universal Drive Manager",
    "Phase 0 prototype · read-only": "Phase 0 프로토타입 · 읽기 전용",
    "Data source": "데이터 소스",
    "Simulated: sample devices": "시뮬레이션: 예제 장치",
    "Simulated: no devices": "시뮬레이션: 장치 없음",
    "Simulated: discovery failure": "시뮬레이션: 검색 실패",
    "Simulated: hot-plug sequence": "시뮬레이션: 연결/분리 순서",
    "This computer (read-only)": "이 컴퓨터 (읽기 전용)",
    "Refreshing…": "새로 고치는 중…",
    "Refresh": "새로 고침",
    "Discover devices again": "장치를 다시 검색합니다",
    "Theme": "테마",
    "System": "시스템",
    "Light": "밝게",
    "Dark": "어둡게",
    "Language": "언어",
    "Showing simulated devices. They do not exist and their capabilities are illustrative.": "시뮬레이션 장치를 표시하고 있습니다. 실제로 존재하지 않으며 기능 상태는 예시입니다.",
    "Showing devices of this computer, discovered read-only.": "이 컴퓨터의 장치를 읽기 전용으로 검색해 표시하고 있습니다.",
    "This build cannot write, format, partition or erase any device.": "이 빌드는 어떤 장치에도 쓰기, 포맷, 파티션 작업, 삭제를 할 수 없습니다.",
    "Devices": "장치",
    "Discovering devices…": "장치를 검색하는 중…",
    "Discovery runs in the background and never writes to a device.": "검색은 백그라운드에서 실행되며 장치에 쓰지 않습니다.",
    "Devices could not be listed": "장치 목록을 가져올 수 없습니다",
    "Try again": "다시 시도",
    "No storage devices found": "저장 장치를 찾지 못했습니다",
    "Connect a device and select Refresh. Virtual devices such as loop devices are not listed.": "장치를 연결한 뒤 새로 고침을 선택하세요. loop 장치 같은 가상 장치는 표시하지 않습니다.",
    "The selected simulated scenario contains no devices.": "선택한 시뮬레이션 시나리오에는 장치가 없습니다.",
    "Simulated device. It does not exist; its identity and capabilities illustrate how real hardware situations are represented.": "시뮬레이션 장치입니다. 실제로 존재하지 않으며, 신원과 기능 정보는 실제 하드웨어 상황을 어떻게 표현하는지 보여 주는 예시입니다.",
    "Device state": "장치 상태",
    "Media": "미디어",
    "Write protection": "쓰기 보호",
    "Removable": "이동식",
    "Yes: mounted, swap or held": "예: 마운트, 스왑 또는 점유됨",
    "No usage detected": "사용 흔적 없음",
    "Block size": "블록 크기",
    "Partitions": "파티션",
    "No partitions reported. Contents are not inspected in this build.": "보고된 파티션이 없습니다. 이 빌드는 내용을 검사하지 않습니다.",
    "Partition {}": "파티션 {}",
    "starts at {}": "시작 위치 {}",
    "Capabilities": "기능",
    'Every operation has an explicit status. \\"Not tested\\" never means supported. * marks operations that can destroy data.':
        '모든 작업에는 명시적인 상태가 있습니다. \\"테스트되지 않음\\"은 지원됨을 뜻하지 않습니다. *는 데이터를 파괴할 수 있는 작업입니다.',
    "Identity evidence": "신원 근거",
    "A serial number or WWN is available. Device paths are never used as identity.": "일련번호 또는 WWN을 사용할 수 있습니다. 장치 경로는 신원으로 사용하지 않습니다.",
    "No reliable serial number or WWN. This device cannot be told apart from an identical one; future destructive operations will require extra confirmation.": "신뢰할 수 있는 일련번호나 WWN이 없습니다. 동일한 장치와 구별할 수 없으므로, 향후 파괴적 작업에는 추가 확인이 필요합니다.",
    "Discovery notes": "검색 참고 사항",
    "No device selected": "선택된 장치 없음",
    "Select a device to see its state, partitions, capabilities and identity evidence.": "장치를 선택하면 상태, 파티션, 기능, 신원 근거를 볼 수 있습니다.",
    "Discovering…": "검색 중…",
    "Discovery failed": "검색 실패",
    "Discovery complete": "검색 완료",
    "No telemetry is collected.": "원격 측정 데이터를 수집하지 않습니다.",
}


def extract():
    strings = []
    for path in sorted(glob.glob(os.path.join(UI, "*.slint"))):
        with open(path, encoding="utf-8") as f:
            source = f.read()
        for m in re.finditer(r'@tr\(\s*"((?:[^"\\]|\\.)*)"', source):
            if m.group(1) not in strings:
                strings.append(m.group(1))
    return strings


def main():
    strings = extract()
    missing = [s for s in strings if s not in KO]
    lines = [
        "# Korean translations for Universal Drive Manager.",
        "# Generated by tools/update_po.py; edit translations there.",
        'msgid ""',
        'msgstr ""',
        '"Content-Type: text/plain; charset=UTF-8\\n"',
        '"Language: ko\\n"',
        '"Plural-Forms: nplurals=1; plural=0;\\n"',
        "",
    ]
    for s in strings:
        lines.append(f'msgid "{s}"')
        lines.append(f'msgstr "{KO.get(s, "")}"')
        lines.append("")
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"{len(strings)} strings, {len(strings) - len(missing)} translated -> {OUT}")
    for s in missing:
        print(f"untranslated: {s}", file=sys.stderr)
    unused = [k for k in KO if k not in strings]
    for s in unused:
        print(f"unused translation: {s}", file=sys.stderr)
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
