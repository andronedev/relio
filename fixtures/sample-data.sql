INSERT INTO content (ContentID, ContentType, MimeType, Title, Attribution, Publisher, DateLastRead, ReadStatus, ___UserID, ___PercentRead, ___FileSize, TimeSpentReading, RestOfBookEstimate, DateAdded, Series, SeriesNumber, Language, IsDownloaded) VALUES
('file:///mnt/onboard/Livres/Candide - Voltaire.kepub.epub', 6, 'application/x-kobo-epub+zip', 'Candide, ou l''Optimisme', 'Voltaire', 'Domaine public', '2026-10-16T21:42:10Z', 2, 'adobe_user', 100, 412880, 14820, 0, '2026-09-02T18:00:00Z', NULL, NULL, 'fr', 'true'),
('file:///mnt/onboard/Livres/Les Miserables T1 - Victor Hugo.kepub.epub', 6, 'application/x-kobo-epub+zip', 'Les Misérables — Fantine', 'Victor Hugo', 'Domaine public', '2026-10-17T07:55:31Z', 1, 'adobe_user', 63, 1830222, 39600, 21400, '2026-09-10T09:12:00Z', 'Les Misérables', '1', 'fr', 'true'),
('file:///mnt/onboard/Livres/Vingt mille lieues - Jules Verne.epub', 6, 'application/epub+zip', 'Vingt mille lieues sous les mers', 'Jules Verne', 'Domaine public', '2026-10-11T22:03:45Z', 1, 'adobe_user', 28, 2240117, 9120, 31800, '2026-09-21T20:30:00Z', 'Voyages extraordinaires', '6', 'fr', 'true'),
('file:///mnt/onboard/Livres/Madame Bovary - Flaubert.kepub.epub', 6, 'application/x-kobo-epub+zip', 'Madame Bovary', 'Gustave Flaubert', 'Domaine public', '2026-08-29T23:10:00Z', 2, 'adobe_user', 100, 980331, 27300, 0, '2026-07-14T10:00:00Z', NULL, NULL, 'fr', 'true'),
('file:///mnt/onboard/Livres/Le Comte de Monte-Cristo - Dumas.kepub.epub', 6, 'application/x-kobo-epub+zip', 'Le Comte de Monte-Cristo', 'Alexandre Dumas', 'Domaine public', NULL, 0, 'adobe_user', 0, 3412009, 0, NULL, '2026-10-15T19:45:00Z', NULL, NULL, 'fr', 'true'),
('file:///mnt/onboard/Livres/Les Fleurs du mal - Baudelaire.epub', 6, 'application/epub+zip', 'Les Fleurs du mal', 'Charles Baudelaire', 'Domaine public', '2026-10-02T12:20:00Z', 1, 'adobe_user', 41, 301554, 3900, 5200, '2026-09-30T08:00:00Z', NULL, NULL, 'fr', 'true'),
('file:///mnt/onboard/Manuels/Rust embarque.pdf', 6, 'application/pdf', NULL, NULL, NULL, '2026-10-14T18:00:00Z', 1, 'adobe_user', 12, 5300120, 2100, NULL, '2026-10-13T17:30:00Z', NULL, NULL, 'en', 'true'),
('8f3c2a1e-store-book-not-downloaded', 6, 'application/x-kobo-epub+zip', 'Livre du store non téléchargé', 'Inconnu', NULL, NULL, 0, 'kobo_user', 0, NULL, 0, NULL, NULL, NULL, NULL, 'fr', 'false'),
('file:///mnt/onboard/Livres/Candide - Voltaire.kepub.epub!OEBPS/ch30.xhtml', 9, 'application/xhtml+xml', 'Chapitre XXX', NULL, NULL, NULL, 0, 'adobe_user', 0, NULL, 0, NULL, NULL, NULL, NULL, NULL, 'true');

INSERT INTO Bookmark (BookmarkID, VolumeID, ContentID, StartContainerPath, StartContainerChildIndex, StartOffset, EndContainerPath, EndContainerChildIndex, EndOffset, Text, Annotation, DateCreated, DateModified, Type, Hidden, ChapterProgress, Color) VALUES
('a1', 'file:///mnt/onboard/Livres/Candide - Voltaire.kepub.epub', 'OEBPS/ch01.xhtml', 'span#kobo\.4\.1', 0, 0, 'span#kobo\.4\.1', 0, 64, 'Tout est pour le mieux dans le meilleur des mondes possibles.', '', '2026-09-03T21:10:00Z', '2026-09-03T21:10:00Z', 'highlight', 'false', 0.05, 0),
('a2', 'file:///mnt/onboard/Livres/Candide - Voltaire.kepub.epub', 'OEBPS/ch30.xhtml', 'span#kobo\.88\.2', 0, 0, 'span#kobo\.88\.2', 0, 30, 'Il faut cultiver notre jardin.', 'La morale du livre : agir plutôt que spéculer.', '2026-10-16T21:40:00Z', '2026-10-16T21:41:00Z', 'note', 'false', 0.99, 2),
('a3', 'file:///mnt/onboard/Livres/Les Miserables T1 - Victor Hugo.kepub.epub', 'OEBPS/part1.xhtml', 'span#kobo\.12\.1', 0, 0, 'span#kobo\.12\.1', 0, 33, 'Aimer ou avoir aimé, cela suffit.', '', '2026-10-01T22:00:00Z', '2026-10-01T22:00:00Z', 'highlight', 'false', 0.42, 1),
('a4', 'file:///mnt/onboard/Livres/Les Miserables T1 - Victor Hugo.kepub.epub', 'OEBPS/part1.xhtml', 'span#kobo\.30\.1', 0, 0, 'span#kobo\.30\.1', 0, 0, NULL, NULL, '2026-10-17T07:55:00Z', '2026-10-17T07:55:00Z', 'dogear', 'false', 0.63, 0),
('a5', 'file:///mnt/onboard/Livres/Vingt mille lieues - Jules Verne.epub', 'OEBPS/ch08.xhtml', 'span#kobo\.2\.1', 0, 0, 'span#kobo\.2\.1', 0, 17, 'Mobilis in mobili', 'La devise du Nautilus.', '2026-10-11T21:58:00Z', '2026-10-11T21:58:00Z', 'note', 'false', 0.21, 3),
('a6', 'file:///mnt/onboard/Livres/Les Fleurs du mal - Baudelaire.epub', 'OEBPS/invitation.xhtml', 'span#kobo\.5\.1', 0, 0, 'span#kobo\.6\.1', 0, 20, 'Là, tout n''est qu''ordre et beauté, Luxe, calme et volupté.', '', '2026-10-02T12:15:00Z', '2026-10-02T12:15:00Z', 'highlight', 'false', 0.38, 0),
('a7', 'file:///mnt/onboard/Livres/Madame Bovary - Flaubert.kepub.epub', 'OEBPS/ch02.xhtml', 'span#kobo\.9\.1', 0, 0, 'span#kobo\.9\.1', 0, 40, 'Surligné puis supprimé', '', '2026-08-01T10:00:00Z', '2026-08-01T10:00:00Z', 'highlight', 'true', 0.1, 0);

INSERT INTO Shelf (CreationDate, Id, InternalName, LastModified, Name, Type, _IsDeleted, _IsVisible, _IsSynced) VALUES
('2026-09-01T10:00:00Z', 'shelf-classiques', 'Classiques', '2026-10-01T10:00:00Z', 'Classiques', 'UserTag', 'false', 'true', 'false'),
('2026-09-01T10:00:00Z', 'shelf-poesie', 'Poésie', '2026-10-01T10:00:00Z', 'Poésie', 'UserTag', 'false', 'true', 'false'),
('2026-09-01T10:00:00Z', 'shelf-old', 'Ancienne', '2026-10-01T10:00:00Z', 'Ancienne', 'UserTag', 'true', 'false', 'false');

INSERT INTO ShelfContent (ShelfName, ContentId, DateModified, _IsDeleted, _IsSynced) VALUES
('Classiques', 'file:///mnt/onboard/Livres/Candide - Voltaire.kepub.epub', '2026-10-01T10:00:00Z', 'false', 'false'),
('Classiques', 'file:///mnt/onboard/Livres/Madame Bovary - Flaubert.kepub.epub', '2026-10-01T10:00:00Z', 'false', 'false'),
('Classiques', 'file:///mnt/onboard/Livres/Le Comte de Monte-Cristo - Dumas.kepub.epub', '2026-10-01T10:00:00Z', 'true', 'false'),
('Poésie', 'file:///mnt/onboard/Livres/Les Fleurs du mal - Baudelaire.epub', '2026-10-01T10:00:00Z', 'false', 'false');
