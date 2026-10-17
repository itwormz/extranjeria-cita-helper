"""Bundle browser runtime resources without Camoufox's separate QML manager."""
from PyInstaller.utils.hooks import collect_all, is_module_or_submodule

excludedimports = ['camoufox.gui']
datas, binaries, hiddenimports = collect_all(
    'camoufox',
    filter_submodules=lambda name: not is_module_or_submodule(name, 'camoufox.gui'),
    exclude_datas=['gui/**'],
)
