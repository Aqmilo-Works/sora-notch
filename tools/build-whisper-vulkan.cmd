@echo off
rem Builds ggml-vulkan.dll, which ships inside Sora.exe (data\converter\whisper-gpu.zip) so transcripts run on the GPU. It
rem comes from the same whisper.cpp release (b5454) as the CPU engine Sora downloads, with the release's own CMake
rem options plus GGML_VULKAN. ggml loads the DLL at run time from the folder Whisper runs in; it must come from the same
rem commit as the engine's ggml-base.dll, so rebuild it whenever whisper-engine changes.
rem Needs: Visual Studio 2022 Build Tools (C++), CMake, the Vulkan SDK (winget: Kitware.CMake, KhronosGroup.VulkanSDK).
rem Usage: build-whisper-vulkan.cmd <empty work folder on a short path, e.g. W:\>   (long paths break MSBuild here)
rem Output: <work>\pkg (ggml-vulkan.dll, LICENSE, BUILD.txt): zip them into data\converter\whisper-gpu.zip and put
rem their SHA-256 in data\converter\whisper-gpu.json (Sora checks them on every use, like ffmpeg).
setlocal
if "%~1"=="" (echo usage: %~nx0 ^<work folder^> & exit /b 1)
set WORK=%~1
for /d %%d in ("C:\VulkanSDK\*") do set VULKAN_SDK=%%d
if not defined VULKAN_SDK (echo Vulkan SDK not found & exit /b 1)
set "PATH=C:\Program Files (x86)\Microsoft Visual Studio\Installer;%PATH%"
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
set "PATH=%VULKAN_SDK%\Bin;C:\Program Files\CMake\bin;%PATH%"
cd /d "%WORK%" || exit /b 1
if not exist src git clone --depth 1 --branch b5454 https://github.com/ggml-org/whisper.cpp src || exit /b 1
cmake -S src -B src\build -A x64 -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON -DWHISPER_SDL2=OFF -DGGML_NATIVE=OFF -DGGML_BACKEND_DL=ON -DGGML_CPU_ALL_VARIANTS=ON -DGGML_VULKAN=ON || exit /b 1
cmake --build src\build --config Release --target ggml-vulkan -j %NUMBER_OF_PROCESSORS% || exit /b 1
if exist pkg rmdir /s /q pkg
mkdir pkg
copy /y src\build\bin\Release\ggml-vulkan.dll pkg\ >nul
copy /y src\LICENSE pkg\LICENSE >nul
for /f %%c in ('git -C src rev-parse HEAD') do set COMMIT=%%c
> pkg\BUILD.txt echo ggml-vulkan.dll from whisper.cpp b5454 (commit %COMMIT%), MIT licence (LICENSE).
>> pkg\BUILD.txt echo Built with tools\build-whisper-vulkan.cmd in Sora: MSVC 2022 x64, Vulkan SDK %VULKAN_SDK%,
>> pkg\BUILD.txt echo -DBUILD_SHARED_LIBS=ON -DGGML_NATIVE=OFF -DGGML_BACKEND_DL=ON -DGGML_CPU_ALL_VARIANTS=ON -DGGML_VULKAN=ON
echo Built %WORK%\pkg
