Attribute VB_Name = "VisiCutR"
' Makros "An VisiCutR senden" fuer CorelDRAW 2017 oder neuer (Windows, VBA).
'
' AddToVisiCutR  fuegt die Zeichnung (oder nur die Auswahl, an ihrer Lage auf
'                der Seite) als Teil zum offenen VisiCutR-Projekt hinzu; ein
'                Teil gleichen Namens wird nur aktualisiert.
' OpenInVisiCutR ersetzt das VisiCutR-Projekt durch die Zeichnung.
'
' Die Seite wird in ein neues Dokument kopiert, Texte dort in Kurven
' umgewandelt und als SVG exportiert; dann startet das Makro
' "visicutr --update [--add] -- datei.svg". Laeuft VisiCutR schon, oeffnet das
' dortige Fenster die Datei. Den Programmpfad legt VisiCutR bei jedem Start in
' %APPDATA%\VisiCutR\visicutr-path.txt ab.
'
' Einrichten: Extras > Makros > Makro-Editor (Alt+F11), im Projekt
' GlobalMacros Datei > Datei importieren ... und diese Datei waehlen.
' Nur ASCII: der Makro-Editor liest Module in der Systemcodierung.
' Teil von VisiCutR, LGPL-3.0-or-later.
Option Explicit

Private Const PATH_FILE As String = "visicutr-path.txt"

Public Sub AddToVisiCutR()
    SendToVisiCutR True
End Sub

Public Sub OpenInVisiCutR()
    SendToVisiCutR False
End Sub

Private Sub SendToVisiCutR(ByVal asPart As Boolean)
    If Documents.Count = 0 Then
        MsgBox T("Kein Dokument ge~offnet."), vbExclamation, "VisiCutR"
        Exit Sub
    End If
    Dim program As String
    program = FindVisiCutR()
    If program = "" Then
        MsgBox T("VisiCutR wurde nicht gefunden. Bitte VisiCutR einmal starten; danach findet das Makro es von selbst."), vbExclamation, "VisiCutR"
        Exit Sub
    End If
    Dim target As String
    target = TemporaryFolder() & "\" & DocumentName(ActiveDocument.Name) & ".svg"
    On Error GoTo Failed
    ExportDrawing ActiveDocument, target
    Dim command As String
    command = Quote(program) & " --update" & IIf(asPart, " --add", "") & " -- " & Quote(target)
    CreateObject("WScript.Shell").Run command, 1, False
    Exit Sub
Failed:
    MsgBox T("An VisiCutR senden fehlgeschlagen: ") & Err.Description, vbCritical, "VisiCutR"
End Sub

' Schreibt die Seite oder die Auswahl als SVG nach target. Ein neues Dokument
' mit gleich grosser Seite nimmt die Objekte an derselben Stelle auf.
Private Sub ExportDrawing(ByVal doc As Document, ByVal target As String)
    Dim source As ShapeRange
    Set source = ActiveSelectionRange
    If source.Count = 0 Then Set source = doc.ActivePage.Shapes.All
    If source.Count = 0 Then Err.Raise vbObjectError + 1, , T("Die Seite ist leer.")

    Dim savedUnit As cdrUnit, scratch As Document
    Dim errorNumber As Long, errorText As String
    savedUnit = doc.Unit
    On Error GoTo Failed
    doc.Unit = cdrMillimeter
    Dim pageBox As Rect, sourceBox As Rect
    Set pageBox = doc.ActivePage.BoundingBox
    Set sourceBox = source.BoundingBox

    Set scratch = CreateDocument()
    scratch.Unit = cdrMillimeter
    scratch.ActivePage.SetSize doc.ActivePage.SizeWidth, doc.ActivePage.SizeHeight
    Dim copied As ShapeRange
    Set copied = source.CopyToLayer(scratch.ActiveLayer)
    Dim boardBox As Rect, copiedBox As Rect
    Set boardBox = scratch.ActivePage.BoundingBox
    Set copiedBox = copied.BoundingBox
    copied.Move (sourceBox.Left - pageBox.Left) - (copiedBox.Left - boardBox.Left), _
        (sourceBox.Bottom - pageBox.Bottom) - (copiedBox.Bottom - boardBox.Bottom)
    Dim texts As ShapeRange
    Set texts = scratch.ActivePage.Shapes.FindShapes(Type:=cdrTextShape)
    If texts.Count > 0 Then texts.ConvertToCurves
    scratch.ExportEx(target, cdrSVG, cdrCurrentPage).Finish

Cleanup:
    On Error Resume Next
    If Not scratch Is Nothing Then
        scratch.Dirty = False
        scratch.Close
    End If
    doc.Unit = savedUnit
    doc.Activate
    On Error GoTo 0
    If errorNumber <> 0 Then Err.Raise errorNumber, , errorText
    Exit Sub
Failed:
    errorNumber = Err.Number
    errorText = Err.Description
    ' Resume beendet die Fehlerbehandlung, sonst griffe Resume Next nicht.
    Resume Cleanup
End Sub

' Programmpfad: VISICUTR_BIN, der von VisiCutR gemerkte Pfad, uebliche Orte.
Private Function FindVisiCutR() As String
    Dim fso As Object, wsh As Object
    Set fso = CreateObject("Scripting.FileSystemObject")
    Set wsh = CreateObject("WScript.Shell")
    Dim candidates(4) As String
    candidates(0) = Variable(wsh, "VISICUTR_BIN")
    candidates(1) = ReadUtf8(fso, Variable(wsh, "APPDATA") & "\VisiCutR\" & PATH_FILE)
    candidates(2) = Variable(wsh, "LOCALAPPDATA") & "\Programs\VisiCutR\visicutr.exe"
    candidates(3) = Variable(wsh, "ProgramFiles") & "\VisiCutR\visicutr.exe"
    candidates(4) = Variable(wsh, "ProgramW6432") & "\VisiCutR\visicutr.exe"
    Dim i As Integer
    For i = 0 To 4
        If Len(candidates(i)) > 0 Then
            If fso.FileExists(candidates(i)) Then
                FindVisiCutR = candidates(i)
                Exit Function
            End If
        End If
    Next i
End Function

' Umgebungsvariable mit Unicode (Environ liefert nur die Systemcodierung).
Private Function Variable(ByVal wsh As Object, ByVal variableName As String) As String
    Dim value As String
    value = wsh.ExpandEnvironmentStrings("%" & variableName & "%")
    If value <> "%" & variableName & "%" Then Variable = value
End Function

Private Function ReadUtf8(ByVal fso As Object, ByVal path As String) As String
    On Error GoTo Done
    If Not fso.FileExists(path) Then Exit Function
    Dim reader As Object
    Set reader = CreateObject("ADODB.Stream")
    reader.Type = 2
    reader.Charset = "utf-8"
    reader.Open
    reader.LoadFromFile path
    ReadUtf8 = Trim$(Replace(Replace(reader.ReadText(-1), vbCr, ""), vbLf, ""))
    reader.Close
Done:
End Function

' Neuer Ordner im Temp-Verzeichnis; VisiCutR liest die Datei beim Neuladen
' erneut, sie bleibt deshalb liegen.
Private Function TemporaryFolder() As String
    Dim fso As Object
    Set fso = CreateObject("Scripting.FileSystemObject")
    Randomize
    Dim folder As String
    folder = fso.GetSpecialFolder(2).Path & "\visicutr-" & Format$(Now, "yyyymmdd-hhnnss") & _
        "-" & CStr(Int(Rnd * 1000000))
    fso.CreateFolder folder
    TemporaryFolder = folder
End Function

' Dateiname ohne Endung, nur Zeichen, die auf jedem System gehen; Umlaute
' ohne Punkte wie in der Inkscape-Erweiterung.
Private Function DocumentName(ByVal title As String) As String
    Dim dot As Long
    dot = InStrRev(title, ".")
    If dot > 1 And Len(title) - dot <= 4 Then title = Left$(title, dot - 1)
    Dim result As String, i As Long, code As Long, c As String
    For i = 1 To Len(title)
        c = Mid$(title, i, 1)
        code = AscW(c)
        Select Case code
            Case 228, 224, 225, 226: result = result & "a"
            Case 246: result = result & "o"
            Case 252: result = result & "u"
            Case 196: result = result & "A"
            Case 214: result = result & "O"
            Case 220: result = result & "U"
            Case 223: result = result & "ss"
            Case 232, 233, 234: result = result & "e"
            Case 48 To 57, 65 To 90, 97 To 122, 32, 45, 46, 95: result = result & c
        End Select
    Next i
    result = Trim$(result)
    Do While Left$(result, 1) = "."
        result = Mid$(result, 2)
    Loop
    Do While Right$(result, 1) = "."
        result = Left$(result, Len(result) - 1)
    Loop
    If result = "" Then result = "Zeichnung"
    DocumentName = result
End Function

Private Function Quote(ByVal text As String) As String
    Quote = """" & text & """"
End Function

' Umlaute in Meldungen: ~a ~o ~u ~s stehen fuer ae, oe, ue, ss mit Punkten.
Private Function T(ByVal text As String) As String
    text = Replace(text, "~a", ChrW(228))
    text = Replace(text, "~o", ChrW(246))
    text = Replace(text, "~u", ChrW(252))
    T = Replace(text, "~s", ChrW(223))
End Function
