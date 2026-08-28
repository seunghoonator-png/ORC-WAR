@echo off
chcp 65001 > nul
title ORC-WAR 전장 스냅샷
echo.
echo  6만 유닛 혼성 전투(보병+창병+궁수+기병)의 스냅샷을 만듭니다.
echo  frame_0000.ppm ~ frame_2200.ppm 이 이 폴더에 생깁니다.
echo  (ppm 은 그림판으로 안 열립니다. 김프나 IrfanView 로 여세요)
echo.
echo  지형을 바꾸려면 마지막 인자를 고치세요:
echo    plains / hills / mountain / river / forest
echo.
orc-war.exe --snapshot 60000 5 . river
echo.
pause
