@echo off
chcp 65001 > nul
title ORC-WAR 성능 측정
echo.
echo  ORC-WAR — 규모별 성능 측정
echo  ============================
echo  시뮬레이션 1틱 예산은 50ms 입니다. 이 안에 들어오면 실시간으로 돌아갑니다.
echo.
echo  [1/4] 5만 유닛
orc-war.exe -n 50000 -t 700 --bench -q
echo.
echo  [2/4] 15만 유닛
orc-war.exe -n 150000 -t 700 --bench -q
echo.
echo  [3/4] 30만 유닛  ← 목표 규모
orc-war.exe -n 300000 -t 700 --bench -q
echo.
echo  [4/4] 50만 유닛  ← 여유 확인용
orc-war.exe -n 500000 -t 700 --bench -q
echo.
echo  측정이 끝났습니다.
pause
