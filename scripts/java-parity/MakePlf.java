/*
 * Writes the VisiCut project files (.plf) of the Java-parity cases with
 * VisiCut itself (VisicutModel.savePlfToStream), so the PLF cases test
 * files as VisiCut writes them. Not part of generate.sh: the PLF is
 * written once and checked in (a ZIP holds time stamps).
 *
 *   java … MakePlf <out.plf> "<file>|<x_mm>|<y_mm>[|<scale>|<degrees>]|<mappings>" …
 *
 * One argument per part; the graphic is placed like in VisiCut's GUI
 * (basic transform, then scale and rotation about the origin, then moved
 * to x/y). <mappings> is "[operation:] filters; …" with the filter syntax
 * of LttParity ("*", "all", "Attribute=value" joined by "&"); the operation
 * is cut, mark, engrave, engrave3d or ignore (a mapping without profile).
 * The profiles are named after their operation.
 *
 * LGPL-3.0-or-later, like VisiCut and LibLaserCut.
 */

import de.thomas_oster.visicut.VisicutModel;
import de.thomas_oster.visicut.managers.MappingManager;
import de.thomas_oster.visicut.model.LaserProfile;
import de.thomas_oster.visicut.model.PlfPart;
import de.thomas_oster.visicut.model.Raster3dProfile;
import de.thomas_oster.visicut.model.RasterProfile;
import de.thomas_oster.visicut.model.VectorProfile;
import de.thomas_oster.visicut.model.mapping.Mapping;
import de.thomas_oster.visicut.model.mapping.MappingSet;
import java.awt.geom.AffineTransform;
import java.io.File;
import java.io.FileOutputStream;
import java.util.ArrayList;

public class MakePlf
{
  public static void main(String[] args) throws Exception
  {
    VisicutModel model = new VisicutModel();
    for (int i = 1; i < args.length; i++)
    {
      String[] f = args[i].split("\\|");
      PlfPart part = model.loadGraphicFile(new File(f[0]), new ArrayList<>());
      double scale = f.length > 4 ? Double.parseDouble(f[3]) : 1;
      double degrees = f.length > 4 ? Double.parseDouble(f[4]) : 0;
      AffineTransform t = AffineTransform.getTranslateInstance(Double.parseDouble(f[1]), Double.parseDouble(f[2]));
      t.rotate(Math.toRadians(degrees));
      t.scale(scale, scale);
      t.concatenate(part.getGraphicObjects().getBasicTransform());
      part.getGraphicObjects().setTransform(t);
      MappingSet mapping = new MappingSet();
      for (String entry : f[f.length - 1].split(";"))
      {
        String[] op = entry.trim().split(":", 2);
        mapping.add(new Mapping(LttParity.filterSet(op[1].trim()), profile(op[0].trim())));
      }
      part.setMapping(mapping);
      model.getPlfFile().add(part);
    }
    try (FileOutputStream out = new FileOutputStream(args[0]))
    {
      model.savePlfToStream(MappingManager.getInstance(), out);
    }
  }

  private static LaserProfile profile(String operation)
  {
    LaserProfile p;
    switch (operation)
    {
      case "ignore":
        return null;
      case "cut":
      case "mark":
      {
        VectorProfile vp = new VectorProfile();
        vp.setIsCut("cut".equals(operation));
        p = vp;
        break;
      }
      case "engrave":
        p = new RasterProfile();
        break;
      case "engrave3d":
        p = new Raster3dProfile();
        break;
      default:
        throw new IllegalArgumentException("unknown operation " + operation);
    }
    p.setName(operation);
    p.setDPI(500);
    return p;
  }
}
