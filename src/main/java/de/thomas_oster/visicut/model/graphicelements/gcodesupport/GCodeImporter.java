/**
 * This file is part of VisiCut.
 * Copyright (C) 2011 - 2013 Thomas Oster <thomas.oster@rwth-aachen.de>
 * RWTH Aachen University - 52062 Aachen, Germany
 *
 *     VisiCut is free software: you can redistribute it and/or modify
 *     it under the terms of the GNU Lesser General Public License as published by
 *     the Free Software Foundation, either version 3 of the License, or
 *     (at your option) any later version.
 *
 *     VisiCut is distributed in the hope that it will be useful,
 *     but WITHOUT ANY WARRANTY; without even the implied warranty of
 *     MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *     GNU Lesser General Public License for more details.
 *
 *     You should have received a copy of the GNU Lesser General Public License
 *     along with VisiCut.  If not, see <http://www.gnu.org/licenses/>.
 **/
package de.thomas_oster.visicut.model.graphicelements.gcodesupport;

import de.thomas_oster.liblasercut.platform.Util;
import de.thomas_oster.visicut.model.graphicelements.AbstractImporter;
import de.thomas_oster.visicut.model.graphicelements.GraphicSet;
import de.thomas_oster.visicut.model.graphicelements.ImportException;
import java.awt.geom.AffineTransform;
import java.awt.geom.Arc2D;
import java.awt.geom.GeneralPath;
import java.io.BufferedReader;
import java.io.File;
import java.io.FileNotFoundException;
import java.io.FileReader;
import java.io.IOException;
import java.util.List;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import javax.swing.filechooser.FileFilter;

/**
 *
 * @author thommy
 */
public class GCodeImporter extends AbstractImporter
{

  private static final FileFilter filter = new FileFilter()
  {

    @Override
    public boolean accept(File file)
    {
      String name = file.getAbsolutePath().toLowerCase();
      return file.isDirectory() || name.endsWith(".nc") || name.endsWith(".gcode");
    }

    @Override
    public String getDescription()
    {
      return "G-Code (.nc, .gcode)";
    }

  };
  
  //current mode 0=move, 1=line, 2=arc cw, 3=arc ccw
  private int g_mode = 0;
  //if using absolute coordinates (G90, default) or relative ones (G91)
  private boolean absolute = true;
  //if I/J are absolute (G90.1) instead of relative to the arc start (G91.1, default)
  private boolean absoluteArcs = false;
  //last position
  private double last_x = 0;
  private double last_y = 0;
  //factor for converting coordinates to mm
  private double unit2mm = 1;
  // a word: letter and number, e.g. G01, X-1.5, Y.25 or F1000
  private Pattern command_pattern = Pattern.compile("[A-Z][-+]?(?:[0-9]+[,.]?[0-9]*|[,.][0-9]+)");

  /**
   * returns the number of a word, e.g. "-1,5" of "X-1,5"
   */
  private double parseNumber(String text)
  {
    text = text.replace(",", ".");
    if (text.endsWith("."))
    {
      text = text.substring(0, text.length() - 1);
    }
    return Double.parseDouble(text);
  }

  /**
   * Appends an arc from the current point around the centre to (x, y),
   * clockwise for G02, counter-clockwise for G03.
   */
  private void appendArc(GeneralPath resultingShape, double cx, double cy, double x, double y, boolean clockwise)
  {
    double radius = Math.sqrt((last_x - cx) * (last_x - cx) + (last_y - cy) * (last_y - cy));
    if (Math.abs(radius - Math.sqrt((x - cx) * (x - cx) + (y - cy) * (y - cy))) >= 0.1)
    {
      System.err.println("GCODE-WARNING: No real circle");
    }
    double start = Math.atan2(last_y - cy, last_x - cx);
    //convert to positive angle
    if (start < 0) start += 2 * Math.PI;
    double end = Math.atan2(y - cy, x - cx);
    //convert to positive angle
    if (end < 0) end += 2 * Math.PI;
    double extend = start - end;
    //convert to negative angle
    if (extend > 0) extend -= 2 * Math.PI;
    Arc2D arc = new Arc2D.Double();
    if (clockwise)
    {
      //strange stuff with the angles, but it seems to work
      arc.setArcByCenter(cx, cy, radius, -Math.toDegrees(start) - 360, Math.toDegrees(extend) + 360, Arc2D.OPEN);
    }
    else
    {
      //no idea why the start coordinate has to be negative, but seems to work
      arc.setArcByCenter(cx, cy, radius, -Math.toDegrees(start), Math.toDegrees(extend), Arc2D.OPEN);
    }
    resultingShape.append(arc, true);
  }

  @Override
  public GraphicSet importSetFromFile(File inputFile, List<String> warnings) throws ImportException
  {
    try (BufferedReader reader = new BufferedReader(new FileReader(inputFile)))
    {
      GraphicSet result = new GraphicSet();
      GeneralPath resultingShape = new GeneralPath();
      resultingShape.moveTo(0, 0);
      String line;
      while ((line = reader.readLine()) != null)
      {
        // comments in parentheses and after a semicolon; words may be lower case
        line = line.replaceAll("\\([^)]*\\)", " ");
        int semicolon = line.indexOf(';');
        if (semicolon >= 0)
        {
          line = line.substring(0, semicolon);
        }
        line = line.toUpperCase();
        Double x = null, y = null, i = null, j = null, r = null;
        boolean actionTriggered = false;
        Matcher matcher = command_pattern.matcher(line);
        while (matcher.find())
        {
          String cmd = matcher.group();
          try
          {
            double value = parseNumber(cmd.substring(1));
            switch (cmd.charAt(0))
            {
              case 'G':
                switch ((int) Math.round(value * 10))
                {
                  case 0: g_mode = 0; break;
                  case 10: g_mode = 1; break;
                  case 20: g_mode = 2; break;
                  case 30: g_mode = 3; break;
                  case 200: unit2mm = Util.inch2mm(1); break;
                  case 210: unit2mm = 1; break;
                  case 900: absolute = true; break;
                  case 910: absolute = false; break;
                  case 901: absoluteArcs = true; break;
                  case 911: absoluteArcs = false; break;
                  default: break;
                }
                break;
              case 'X': x = unit2mm * value; actionTriggered = true; break;
              case 'Y': y = unit2mm * value; actionTriggered = true; break;
              case 'I': i = unit2mm * value; break;
              case 'J': j = unit2mm * value; break;
              case 'R': r = unit2mm * value; break;
              default: break;
            }
          }
          catch (NumberFormatException e)
          {
            warnings.add("Illegal G-Code: " + cmd);
          }
        }
        if (!actionTriggered)
        {
          continue;
        }
        // a missing coordinate keeps its value (absolute) or does not move (relative)
        double tx = absolute ? (x != null ? x : last_x) : last_x + (x != null ? x : 0);
        double ty = absolute ? (y != null ? y : last_y) : last_y + (y != null ? y : 0);
        if (g_mode == 0)
        {
          resultingShape.moveTo(tx, ty);
        }
        else if (g_mode == 1)
        {
          resultingShape.lineTo(tx, ty);
        }
        else
        {
          double cx, cy;
          if (r != null)
          {
            // centre from the radius: a positive radius takes the shorter arc,
            // a negative one the longer (as grbl does)
            double dx = tx - last_x;
            double dy = ty - last_y;
            double chord = Math.sqrt(dx * dx + dy * dy);
            if (chord < 1e-9)
            {
              warnings.add("G-Code: full circle with R is ambiguous, skipped");
              continue;
            }
            double height = 4 * r * r - chord * chord;
            if (height < 0)
            {
              if (2 * Math.abs(r) - chord < -0.1)
              {
                warnings.add("G-Code: arc radius smaller than half the distance of its end points");
              }
              height = 0;
            }
            double factor = Math.sqrt(height) / chord;
            if (g_mode == 2)
            {
              factor = -factor;
            }
            if (r < 0)
            {
              factor = -factor;
            }
            cx = last_x + 0.5 * (dx - dy * factor);
            cy = last_y + 0.5 * (dy + dx * factor);
          }
          else if (absoluteArcs)
          {
            cx = i != null ? i : last_x;
            cy = j != null ? j : last_y;
          }
          else
          {
            cx = last_x + (i != null ? i : 0);
            cy = last_y + (j != null ? j : 0);
          }
          appendArc(resultingShape, cx, cy, tx, ty, g_mode == 2);
        }
        last_x = tx;
        last_y = ty;
      }
      result.add(new GCodeShape(resultingShape, inputFile));
      result.setBasicTransform(new AffineTransform());
      result.setTransform(result.getBasicTransform());
      return result;
    }
    catch (FileNotFoundException ex)
    {
      throw new ImportException(ex);
    }
    catch (IOException ex)
    {
      throw new ImportException(ex);
    }
  }

  public FileFilter getFileFilter()
  {
    return filter;
  }
  
}
