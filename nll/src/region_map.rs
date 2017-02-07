#![allow(dead_code)]

use env::Point;
use nll_repr::repr;
use region::Region;
use std::collections::HashMap;

pub struct RegionMap {
    num_vars: usize,
    use_constraints: Vec<(RegionVariable, Point)>,
    flow_constraints: Vec<(RegionVariable, Point, Point)>,
    outlive_constraints: Vec<(RegionVariable, RegionVariable)>,
    user_region_names: HashMap<repr::RegionName, Vec<RegionVariable>>,
    region_eq_assertions: Vec<(repr::RegionName, Region)>,
    region_in_assertions: Vec<(repr::RegionName, Point, bool)>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RegionVariable {
    index: usize,
}

pub struct UseConstraint {
    var: RegionVariable,
    contains: Point,
}

pub struct InAssertion {
    var: RegionVariable,
    contains: Point,
}

pub struct OutAssertion {
    var: RegionVariable,
    contains: Point,
}

impl RegionMap {
    pub fn new() -> Self {
        RegionMap {
            num_vars: 0,
            use_constraints: vec![],
            flow_constraints: vec![],
            outlive_constraints: vec![],
            region_eq_assertions: vec![],
            region_in_assertions: vec![],
            user_region_names: HashMap::new(),
        }
    }

    pub fn new_var(&mut self) -> RegionVariable {
        self.num_vars += 1;
        RegionVariable { index: self.num_vars - 1 }
    }

    pub fn instantiate_ty<T>(&mut self, ty: &repr::Ty<T>) -> repr::Ty<RegionVariable> {
        repr::Ty {
            name: ty.name,
            args: ty.args.iter().map(|a| self.instantiate_arg(a)).collect(),
        }
    }

    fn instantiate_arg<T>(&mut self, arg: &repr::TyArg<T>) -> repr::TyArg<RegionVariable> {
        match *arg {
            repr::TyArg::Region(_) => repr::TyArg::Region(self.new_var()),
            repr::TyArg::Ty(ref t) => repr::TyArg::Ty(self.instantiate_ty(t)),
        }
    }

    pub fn use_ty(&mut self, ty: &repr::Ty<RegionVariable>, point: Point) {
        for_each_region_variable(ty, &mut |var| self.use_constraints.push((var, point)));
    }

    pub fn user_names(&mut self, rn: repr::RegionName, ty: &repr::Ty<RegionVariable>) {
        let mut regions = vec![];
        for_each_region_variable(ty, &mut |var| regions.push(var));
        self.user_region_names.insert(rn, regions);
        log!("user_names: rn={:?} ty={:?}", rn, ty);
    }

    pub fn assert_region_eq(&mut self, name: repr::RegionName, region: Region) {
        self.region_eq_assertions.push((name, region));
    }

    pub fn assert_region_contains(&mut self,
                                  name: repr::RegionName,
                                  point: Point,
                                  expected: bool) {
        self.region_in_assertions.push((name, point, expected));
    }

    pub fn flow(&mut self, a_ty: &repr::Ty<RegionVariable>, a_point: Point, b_point: Point) {
        for_each_region_variable(a_ty,
                                 &mut |var| self.flow_constraints.push((var, a_point, b_point)));
    }

    /// Create the constraints such that `sub_ty <: super_ty`. Here we
    /// assume that both types are instantiations of a common 'erased
    /// type skeleton', and hence that the regions we will encounter
    /// as we iterate line up prefectly.
    ///
    /// We also assume all regions are contravariant for the time
    /// being.
    pub fn subtype(&mut self, a_ty: &repr::Ty<RegionVariable>, b_ty: &repr::Ty<RegionVariable>) {
        let mut a_regions = vec![];
        for_each_region_variable(a_ty, &mut |var| a_regions.push(var));

        let mut b_regions = vec![];
        for_each_region_variable(b_ty, &mut |var| b_regions.push(var));

        assert_eq!(a_regions.len(), b_regions.len());

        for (&a_region, &b_region) in a_regions.iter().zip(&b_regions) {
            self.outlive_constraints.push((a_region, b_region));
        }
    }

    pub fn solve<'m>(&'m self) -> RegionSolution<'m> {
        RegionSolution::new(self)
    }
}

pub fn for_each_region_variable<OP>(ty: &repr::Ty<RegionVariable>, op: &mut OP)
    where OP: FnMut(RegionVariable)
{
    for arg in &ty.args {
        for_each_region_variable_in_arg(arg, op);
    }
}

fn for_each_region_variable_in_arg<OP>(arg: &repr::TyArg<RegionVariable>, op: &mut OP)
    where OP: FnMut(RegionVariable)
{
    match *arg {
        repr::TyArg::Ty(ref t) => for_each_region_variable(t, op),
        repr::TyArg::Region(var) => op(var),
    }
}

pub struct RegionSolution<'m> {
    region_map: &'m RegionMap,
    values: Vec<Region>,
}

impl<'m> RegionSolution<'m> {
    pub fn new(region_map: &'m RegionMap) -> Self {
        let mut solution = RegionSolution {
            region_map: region_map,
            values: (0..region_map.num_vars).map(|_| Region::new()).collect(),
        };
        solution.find();
        solution
    }

    fn find(&mut self) {
        for &(var, point) in &self.region_map.use_constraints {
            self.values[var.index].add_point(point);
            log!("user_constraints: var={:?} value={:?} point={:?}",
                 var,
                 self.values[var.index],
                 point);
        }

        let mut changed = true;
        while changed {
            changed = false;

            // Data in region R flows from point A to point B (without changing
            // name). Therefore, if it is used in B, A must in R.
            for &(a, a_point, b_point) in &self.region_map.flow_constraints {
                if self.values[a.index].contains(b_point) {
                    changed |= self.values[a.index].add_point(a_point);
                }
            }

            // 'a: 'b -- add everything 'b into 'a
            for &(a, b) in &self.region_map.outlive_constraints {
                assert!(a != b);

                log!("outlive_constraints: a={:?} a_value={:?}",
                     a,
                     self.values[a.index]);
                log!("                       b={:?} b_value={:?}",
                     b,
                     self.values[b.index]);

                // In any case, A must include all points in B.
                let b_value = self.values[b.index].clone();
                changed |= self.values[a.index].add_region(&b_value);
            }
        }
    }

    pub fn region(&self, var: RegionVariable) -> &Region {
        &self.values[var.index]
    }

    pub fn check(&self) -> usize {
        let mut errors = 0;

        for &(user_region, ref expected_region) in &self.region_map.region_eq_assertions {
            for &region_var in &self.region_map.user_region_names[&user_region] {
                let actual_region = self.region(region_var);
                if actual_region != expected_region {
                    println!("error: region `{:?}` came to `{:?}`, which was not expected",
                             user_region,
                             actual_region);
                    println!("    expected `{:?}`", expected_region);
                    errors += 1;
                }
            }
        }

        for &(user_region, point, expected) in &self.region_map.region_in_assertions {
            for &region_var in &self.region_map.user_region_names[&user_region] {
                let actual_region = self.region(region_var);
                let contained = actual_region.contains(point);
                if expected && !contained {
                    println!("error: region `{:?}` did not contain `{:?}` as it should have",
                             user_region, point);
                    println!("    actual region `{:?}`", actual_region);
                    errors += 1;
                } else if !expected && contained {
                    println!("error: region `{:?}` contained `{:?}`, which it should not have",
                             user_region, point);
                    println!("    actual region `{:?}`", actual_region);
                } else {
                    assert_eq!(expected, contained);
                }
            }
        }

        errors
    }
}
