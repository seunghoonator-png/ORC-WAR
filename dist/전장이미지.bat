@echo off
chcp 65001 > nul
title ORC-WAR 전장 스냅샷
echo.
echo  6만 유닛 전투를 돌리며 전장 스냅샷을 만듭니다.
echo  frame_0000.ppm ~ frame_2600.ppm 파일이 이 폴더에 생깁니다.
echo  (ppm 은 그림판으로는 안 열립니다. 김프나 IrfanView 로 여세요)
echo.
snapshot.exe 60000 1 .
echo.
pause
