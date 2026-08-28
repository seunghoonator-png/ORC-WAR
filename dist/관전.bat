@echo off
chcp 65001 > nul
title ORC-WAR 관전
echo.
echo   ORC-WAR — 무엇을 보시겠습니까
echo   ================================
echo    1  야전     8만  (보병 창병 궁수 기병)
echo    2  야전    20만
echo    3  야전    30만  ← 목표 규모
echo    4  언덕     8만
echo    5  도하전   8만  (강과 숲)
echo    6  산악     8만
echo    7  공성전   6만
echo.
set /p pick=번호를 고르세요 (그냥 엔터면 1):
if "%pick%"=="" set pick=1
if "%pick%"=="1" start "" watch.exe field 80000
if "%pick%"=="2" start "" watch.exe field 200000
if "%pick%"=="3" start "" watch.exe field 300000
if "%pick%"=="4" start "" watch.exe hills 80000
if "%pick%"=="5" start "" watch.exe river 80000
if "%pick%"=="6" start "" watch.exe mountain 80000
if "%pick%"=="7" start "" watch.exe siege 60000
echo.
echo   창이 열립니다. 조작법:
echo     스페이스   일시정지
echo     [ ]        배속 내리기 / 올리기  (0.5 ~ 16배)
echo     WASD 화살표 카메라 이동  (마우스 드래그도 됩니다)
echo     휠 또는 QE  확대 / 축소
echo     F          전장 전체 보기
echo     R          같은 설정으로 다시
echo     Esc        종료
echo.
echo   처음에는 전장 전체가 보입니다. 휠로 당겨 보시면 병사 하나하나가 보입니다.
echo.
timeout /t 8 > nul
