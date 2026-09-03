#!/usr/bin/env python3
"""창을 띄우고 키를 넣어 화면 흐름을 끝까지 확인한다.

이 개발 환경에는 화면이 없다. 그래서 화면 코드는 대부분 화소 버퍼로 구워
검사하지만, **키를 눌렀을 때 화면이 어떻게 넘어가는가**는 그 방법으로 잡히지
않는다. 실제로 창을 띄우고 사람처럼 키를 눌러 봐야 한다.

실제로 이걸로 하나 잡았다 — 사람이 키를 한 번 누르면 60fps 화면에서는 대여섯
프레임 동안 눌린 상태다. 그 사이 화면이 넘어가면 다음 화면이 같은 누름을
자기 것으로 받는다. 전투 끝에 ENTER 를 한 번 눌렀는데 결과 화면을 지나쳐
설정으로 돌아갔고, ESC 를 한 번 눌렀는데 결과를 보지도 못하고 닫혔다.

    Xvfb :79 -screen 0 1600x900x24 &
    (cd dist && DISPLAY=:79 wine orc-war.exe &)
    python3 tools/drive_window.py :79 \\
        Down 1 Left 2 Return 1 wait 8 bracketright 4 \\
        wait 80 shot /tmp/verdict.png Return 1 wait 3 shot /tmp/report.png

인자는 (키 이름, 횟수) 쌍이며, `wait 초` 와 `shot 경로` 를 섞어 쓴다.
필요한 것: python-xlib, ImageMagick(import). 창 매니저가 없으면 포커스가
포인터를 따라가므로 먼저 커서를 창 한복판으로 옮긴다.
"""
import subprocess
import sys
import time

from Xlib import X, XK, display
from Xlib.ext import xtest


def main() -> None:
    d = display.Display(sys.argv[1])
    # 창 매니저가 없으면 포커스는 커서를 따라간다
    xtest.fake_input(d, X.MotionNotify, x=760, y=430)
    d.sync()

    def key(name: str, times: int) -> None:
        code = d.keysym_to_keycode(XK.string_to_keysym(name))
        for _ in range(times):
            xtest.fake_input(d, X.KeyPress, code)
            d.sync()
            time.sleep(0.06)
            xtest.fake_input(d, X.KeyRelease, code)
            d.sync()
            time.sleep(0.25)

    argv = sys.argv[2:]
    i = 0
    while i < len(argv):
        cmd, arg = argv[i], argv[i + 1]
        if cmd == "wait":
            time.sleep(float(arg))
        elif cmd == "shot":
            subprocess.run(
                ["import", "-display", sys.argv[1], "-window", "root", arg], check=True
            )
            print("shot", arg, flush=True)
        else:
            key(cmd, int(arg))
            print("key", cmd, arg, flush=True)
        i += 2


if __name__ == "__main__":
    main()
