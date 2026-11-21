# CopieP

Fork de [CopyQ](https://github.com/hluk/CopyQ) pour Mac à puce Apple, en français.

## Ce qui change par rapport à CopyQ

- **Éléments épinglés protégés** : leur contenu est masqué (« 🔒 Élément protégé »).
  Entrée ou double-clic demande Touch ID (ou le mot de passe du Mac), puis colle l'élément.
  Ils se reverrouillent dès que l'app perd le focus. Les scripts et le menu de la barre
  ne voient jamais leur contenu.
- **Option + V** : petite fenêtre sous la souris pour choisir quoi coller, façon Windows 11.
- **Look macOS** : nouvelle icône, icône monochrome dans la barre de menus, lignes arrondies,
  sans numéros ni rayures, barre d'outils masquée.
- **Français par défaut.**

## Installer sur ce Mac (double-clic)

1. Récupérez le code : bouton vert **Code → Download ZIP** sur GitHub, puis décompressez
   (ou `git clone https://github.com/gardy-doob/CopieP.git` pour avoir les mises à jour).
2. Double-cliquez **`INSTALLER-SUR-CE-MAC.command`**. Si macOS bloque : clic droit → Ouvrir.

Le script installe ce qui manque (outils Apple, Homebrew, Qt), compile, remplace l'app dans
Applications et la lance. Première fois : 20 à 30 min. Ensuite : 1 à 2 min.

Avec un dossier cloné par git, le script récupère aussi la dernière version du code.

## Version compilée par GitHub

Chaque modification poussée sur `master` lance la compilation dans l'onglet **Actions**.
Le .dmg se télécharge en bas de la page du build (section *Artifacts*). Après l'avoir
glissé dans Applications :

```
xattr -cr /Applications/CopyQ.app
codesign --force --deep --sign - /Applications/CopyQ.app
```

## Licence

GPL-3.0, comme CopyQ.
