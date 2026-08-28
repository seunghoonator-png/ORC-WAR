@echo off
chcp 65001 > nul
title ORC-WAR 지형별 전투
echo.
echo  같은 병력(검방보병 2000 동수)을 지형만 바꿔 붙입니다
echo  ======================================================
echo.
echo  [평지]  숨을 곳도 막을 곳도 없는 정면 대결
orc-war.exe --matchup 0 2000 0 2000 200 1 plains
echo.
echo  [언덕]  전장 한복판을 능선이 가로지릅니다
orc-war.exe --matchup 0 2000 0 2000 200 1 hills
echo.
echo  [강]  여울 몇 곳으로만 건널 수 있습니다
orc-war.exe --matchup 0 2000 0 2000 200 1 river
echo.
echo  [숲]  대열이 풀리고 화살이 가지에 걸립니다
orc-war.exe --matchup 0 2000 0 2000 200 1 forest
echo.
echo  [산악]  협곡 통로가 전부입니다. 전사자를 평지와 비교해 보세요
orc-war.exe --matchup 0 2000 0 2000 200 1 mountain
echo.
pause
