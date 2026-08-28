@echo off
chcp 65001 > nul
title ORC-WAR 자체 점검
echo.
echo  이 PC 에서 제대로 도는지 확인합니다. 1~2분 걸립니다.
echo  ==================================================
echo  같은 씨앗이면 같은 전투가 나오는지, 전투가 결판까지 가는지,
echo  성벽이 실제로 부서지는지, 30만이 실시간으로 도는지를 봅니다.
echo.
orc-war.exe --selftest
