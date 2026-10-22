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
package de.thomas_oster.visicut.model.graphicelements.jpgpngsupport;

import de.thomas_oster.liblasercut.platform.Util;
import de.thomas_oster.visicut.model.graphicelements.AbstractImporter;
import de.thomas_oster.visicut.model.graphicelements.GraphicSet;
import de.thomas_oster.visicut.model.graphicelements.ImportException;
import java.awt.geom.AffineTransform;
import java.awt.image.BufferedImage;
import java.io.File;
import java.io.IOException;
import java.util.Iterator;
import java.util.List;
import javax.imageio.ImageIO;
import javax.imageio.ImageReader;
import javax.imageio.metadata.IIOMetadata;
import javax.imageio.metadata.IIOMetadataFormatImpl;
import javax.imageio.stream.ImageInputStream;
import org.w3c.dom.Node;
import javax.swing.filechooser.FileFilter;

/**
 *
 * @author Thomas Oster <thomas.oster@rwth-aachen.de>
 */
public class JPGPNGImporter extends AbstractImporter
{

  public GraphicSet importSetFromFile(File inputFile, List<String> warnings) throws ImportException
  {
    try
    {
      GraphicSet result = new GraphicSet();
      // resolution stored in the file (PNG pHYs, JPEG JFIF or Exif, BMP), 72 DPI otherwise
      double[] dpi = resolutionDpi(inputFile);
      if (dpi == null)
      {
        dpi = new double[]{72, 72};
      }
      result.setBasicTransform(AffineTransform.getScaleInstance(Util.inch2mm(1d / dpi[0]), Util.inch2mm(1d / dpi[1])));
      BufferedImage image = ImageIO.read(inputFile);
      if (image != null)
      {
        result.add(new JPGPNGImage(image));
      }
      else
      {
        throw new ImportException("Image file is damaged or has unsupported format.");
      }

      return result;
    }
    catch (IOException ex)
    {
      throw new ImportException(ex);
    }
  }

  /**
   * Horizontal and vertical resolution in DPI from the image metadata, or null.
   */
  private static double[] resolutionDpi(File file)
  {
    try (ImageInputStream input = ImageIO.createImageInputStream(file))
    {
      if (input == null)
      {
        return null;
      }
      Iterator<ImageReader> readers = ImageIO.getImageReaders(input);
      if (!readers.hasNext())
      {
        return null;
      }
      ImageReader reader = readers.next();
      try
      {
        reader.setInput(input);
        IIOMetadata metadata = reader.getImageMetadata(0);
        if (metadata == null || !metadata.isStandardMetadataFormatSupported())
        {
          return null;
        }
        Node tree = metadata.getAsTree(IIOMetadataFormatImpl.standardMetadataFormatName);
        Double horizontal = pixelSizeMm(tree, "HorizontalPixelSize");
        Double vertical = pixelSizeMm(tree, "VerticalPixelSize");
        if (horizontal == null || vertical == null)
        {
          return null;
        }
        double[] dpi = {snapDpi(25.4 / horizontal), snapDpi(25.4 / vertical)};
        for (double value : dpi)
        {
          if (!(value >= 1 && value <= 100000))
          {
            return null;
          }
        }
        return dpi;
      }
      finally
      {
        reader.dispose();
      }
    }
    catch (IOException | RuntimeException ex)
    {
      return null;
    }
  }

  /** Files store whole pixels per metre: 2835 px/m mean 72 DPI, not 72.009. */
  private static double snapDpi(double dpi)
  {
    return Math.abs(dpi - Math.round(dpi)) < 0.05 ? Math.round(dpi) : dpi;
  }

  /** The "value" attribute of the named node of the standard metadata tree, in mm. */
  private static Double pixelSizeMm(Node node, String name)
  {
    if (name.equals(node.getNodeName()) && node.getAttributes() != null)
    {
      Node value = node.getAttributes().getNamedItem("value");
      if (value != null)
      {
        try
        {
          double size = Double.parseDouble(value.getNodeValue());
          return size > 0 ? size : null;
        }
        catch (NumberFormatException ex)
        {
          return null;
        }
      }
    }
    for (Node child = node.getFirstChild(); child != null; child = child.getNextSibling())
    {
      Double found = pixelSizeMm(child, name);
      if (found != null)
      {
        return found;
      }
    }
    return null;
  }

  public FileFilter getFileFilter()
  {
    return new FileFilter()
    {

      @Override
      public boolean accept(File file)
      {
        String name = file.getAbsolutePath().toLowerCase();
        return file.isDirectory() || name.endsWith(".png") || name.endsWith(".jpg") || name.endsWith(".bmp") || name.endsWith(".jpeg");
      }

      @Override
      public String getDescription()
      {
        return "Rastergrafiken (jpg,png,bmp)";
      }

    };
  }

}
