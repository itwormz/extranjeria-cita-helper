"""Keep the complete runtime driver, excluding developer TypeScript declarations."""
from PyInstaller.utils.hooks import collect_all

datas, binaries, hiddenimports = collect_all(
    'playwright',
    exclude_datas=['driver/package/types/**', 'driver/package/**/*.d.ts'],
)
