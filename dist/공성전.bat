@echo off
title ORC-WAR 공성전
echo.
echo  성을 두고 벌어지는 전투입니다.
echo  ================================
echo  성벽은 세 가지 방법으로 뚫립니다. 투석기로 구간을 무너뜨리거나,
echo  파성추로 성문을 부수거나, 사다리를 걸고 기어오르는 것입니다.
echo.
echo  [수비 3000]  공격군 8000 이 성을 칩니다
orc-war.exe --siege 8000 3000 1
echo.
echo  [수비 4500]  같은 공격군, 수비만 1.5배
orc-war.exe --siege 8000 4500 1
echo.
echo  수비를 1.5배로 늘리면 공격측 손실이 얼마나 뛰는지 비교해 보세요.
echo  직접 해보려면:  orc-war.exe --siege [공격] [수비] [시드]
echo  해자 없이:      orc-war.exe --siege 8000 3000 1 nomoat
echo.
pause
