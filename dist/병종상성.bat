@echo off
chcp 65001 > nul
title ORC-WAR 병종 상성
echo.
echo  병종 상성 — 각 600명씩 동수로 붙입니다
echo  =========================================
echo.
echo  [중기병 vs 궁수]  개활지에서 기병은 사수에게 재앙입니다
matchup.exe 6 600 3 600
echo.
echo  [중기병 vs 장창병]  자리를 지킨 창벽에 정면 돌격하면 갈립니다
matchup.exe 6 600 1 600
echo.
echo  [중기병 vs 검방보병]  방패를 든 밀집 보병과는 호각입니다
matchup.exe 6 600 0 600
echo.
echo  [궁수 vs 검방보병]  화살통이 비면 근접전이 됩니다
matchup.exe 3 600 0 600
echo.
echo  [궁수 vs 중갑도끼병]  갑옷은 화살을 견딥니다
matchup.exe 3 600 2 600
echo.
echo  [경기병 vs 검방보병]  경기병은 전열을 뚫는 병종이 아닙니다
matchup.exe 5 600 0 600
echo.
echo  병종 번호: 0 검방보병 / 1 장창병 / 2 중갑도끼 / 3 궁수
echo             4 석궁수 / 5 경기병 / 6 중기병 / 7 궁기병
echo  직접 붙여보려면:  matchup.exe [병종A] [수A] [병종B] [수B]
echo.
pause
