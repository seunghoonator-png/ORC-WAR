@echo off
title ORC-WAR 성능 측정
echo.
echo  ORC-WAR - 규모별 성능 측정
echo  ============================
echo  시뮬레이션 1틱 예산은 50ms 입니다. 이 안에 들어오면 실시간으로 돌아갑니다.
echo  "예산 초과 몇 %%" 줄이 0 에 가까울수록 화면이 매끄럽습니다.
echo.
echo  [1/4] 야전 5만
orc-war.exe --bench --field plains -n 50000 -t 700 -q
echo.
echo  [2/4] 야전 15만
orc-war.exe --bench --field plains -n 150000 -t 700 -q
echo.
echo  [3/4] 야전 30만  <- 목표 규모
orc-war.exe --bench --field plains -n 300000 -t 900 -q
echo.
echo  [4/4] 공성전 30만  <- 가장 무거운 경우
orc-war.exe --bench --field siege -n 300000 -t 900 -q
echo.
echo  측정이 끝났습니다.
pause
